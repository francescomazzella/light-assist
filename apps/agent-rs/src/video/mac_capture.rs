//! Screen capture through ScreenCaptureKit.
//!
//! macOS shrinks the picture and converts it to the encoder's format itself, and hands each one over
//! as a buffer on the graphics card that the encoder takes as it is.

use std::{
    ffi::c_void,
    sync::{Arc, Mutex},
    time::Duration,
};

use block2::RcBlock;
use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, DefinedClass};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType, SCWindow,
};

use super::mac_monitors::Monitor;
use crate::{
    mac::{may_record_screen, windows_on_screen, CFRetain, CFStringRef, CFTypeRef, Cf},
    util::lock,
};

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMSampleBufferGetImageBuffer(sample: *const c_void) -> CFTypeRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGDisplayStreamYCbCrMatrix_ITU_R_709_2: CFStringRef;
}

/// `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`: NV12 with limited (16 to 235) range, which is
/// what browsers assume for HD video and what the encoder takes.
const NV12_VIDEO_RANGE: u32 = u32::from_be_bytes(*b"420v");
/// How long macOS is given to answer before the capture is given up on for now.
const ANSWER_WITHIN: Duration = Duration::from_secs(5);
/// How many pictures macOS may have on their way at once. One is held here while it is encoded.
const QUEUE_DEPTH: isize = 5;
/// `SCStreamErrorUserStopped`: the sharer ended the capture from macOS's own menu bar control.
const USER_STOPPED: isize = -3817;

/// Lets something of Apple's that is safe to hand to another thread cross to it.
struct Carried<T>(T);
unsafe impl<T> Send for Carried<T> {}

/// What the capture's own thread leaves for the thread that encodes.
#[derive(Default)]
struct Shared {
    /// The newest picture, a `CVPixelBuffer`, and whether it has been collected yet.
    latest: Mutex<(Option<Cf>, bool)>,
    /// Why macOS stopped the capture, once it has, and whether it was the sharer who asked it to.
    stopped: Mutex<Option<(String, bool)>>,
}

define_class!(
    // SAFETY: `NSObject` has no rules for its subclasses, and this one has no `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "LightAssistScreenOutput"]
    #[ivars = Arc<Shared>]
    struct Output;

    unsafe impl NSObjectProtocol for Output {}

    unsafe impl SCStreamOutput for Output {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            // While nothing on screen changes, macOS sends word of that without a picture.
            let picture = CMSampleBufferGetImageBuffer(sample as *const CMSampleBuffer as *const c_void);
            if !picture.is_null() {
                *lock(&self.ivars().latest) = (Some(Cf(CFRetain(picture))), true);
            }
        }
    }

    unsafe impl SCStreamDelegate for Output {
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn stream_did_stop(&self, _stream: &SCStream, error: &NSError) {
            *lock(&self.ivars().stopped) = Some((error.localizedDescription().to_string(), error.code() == USER_STOPPED));
        }
    }
);

impl Output {
    fn new(shared: Arc<Shared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        unsafe { msg_send![super(this), init] }
    }
}

/// The largest even-sided size with the source's shape that fits inside `max`.
fn fit(source: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let (width, height) = (source.0 as u64, source.1 as u64);
    let (mut fit_width, mut fit_height) = (width, height);
    if fit_width > max.0 as u64 {
        (fit_width, fit_height) = (max.0 as u64, height * max.0 as u64 / width);
    }
    if fit_height > max.1 as u64 {
        (fit_width, fit_height) = (width * max.1 as u64 / height, max.1 as u64);
    }
    ((fit_width as u32 & !1).max(2), (fit_height as u32 & !1).max(2))
}

/// Waits for macOS to say how something asked of a stream went.
fn answered(ask: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
    let (done, answer) = std::sync::mpsc::sync_channel(1);
    let block = RcBlock::new(move |error: *mut NSError| {
        let _ = done.try_send(unsafe { error.as_ref() }.map(|error| error.localizedDescription().to_string()));
    });
    ask(&block);
    match answer.recv_timeout(ANSWER_WITHIN) {
        Ok(None) => Ok(()),
        Ok(Some(error)) => Err(error),
        Err(_) => Err("macOS did not answer".to_owned()),
    }
}

/// What there is on screen to capture, as macOS lists it.
fn shareable() -> Result<Retained<SCShareableContent>, String> {
    let (done, answer) = std::sync::mpsc::sync_channel(1);
    let block = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        let content = unsafe { Retained::retain(content) }.map(Carried);
        let _ = done.try_send(content.ok_or_else(|| unsafe { error.as_ref() }.map_or("nothing to capture".to_owned(), |error| error.localizedDescription().to_string())));
    });
    unsafe { SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(false, true, &block) };
    answer.recv_timeout(ANSWER_WITHIN).map_err(|_| "macOS did not answer".to_owned())?.map(|content| content.0)
}

/// The app's own windows that stay above everything else: the indicator that a helper is connected,
/// which the helper is not shown. Its console is an ordinary window and stays in the picture.
fn own_overlays() -> Vec<u32> {
    let mut overlays: Vec<u32> = windows_on_screen().iter().filter(|window| window.owner == std::process::id() as i32 && window.layer != 0).map(|window| window.id).collect();
    overlays.sort_unstable();
    overlays
}

/// Everything on `display` except this app's own overlays.
unsafe fn filter(content: &SCShareableContent, display: &SCDisplay) -> Retained<SCContentFilter> {
    let overlays: Vec<Retained<SCWindow>> = content
        .windows()
        .iter()
        .filter(|window| window.windowLayer() != 0 && window.owningApplication().is_some_and(|app| app.processID() == std::process::id() as i32))
        .collect();
    SCContentFilter::initWithDisplay_excludingWindows(SCContentFilter::alloc(), display, &NSArray::from_retained_slice(&overlays))
}

fn interval(fps: u32) -> CMTime {
    CMTime { value: 1, timescale: fps.max(1) as i32, flags: CMTimeFlags::Valid, epoch: 0 }
}

/// One display, read into NV12 pictures no larger than the size asked for.
pub struct Screen {
    stream: Retained<SCStream>,
    configuration: Retained<SCStreamConfiguration>,
    /// Kept alive for as long as the stream may call it.
    _output: Retained<Output>,
    shared: Arc<Shared>,
    display: u32,
    size: (u32, u32),
    /// The overlays left out of the picture, as they were when the capture was last told.
    left_out: Vec<u32>,
}

impl Screen {
    pub fn open(monitor: &Monitor, max: (u32, u32), fps: u32, cursor: bool) -> Result<Self, String> {
        if !may_record_screen() {
            return Err("macOS has not been told to let Light Assist record the screen".to_owned());
        }
        objc2::rc::autoreleasepool(|_| unsafe {
            let left_out = own_overlays();
            let content = shareable()?;
            let display = content.displays().iter().find(|display| display.displayID() == monitor.display).ok_or("the display is gone")?;
            let size = fit((monitor.width.max(2) as u32, monitor.height.max(2) as u32), max);

            let configuration = SCStreamConfiguration::new();
            configuration.setWidth(size.0 as usize);
            configuration.setHeight(size.1 as usize);
            configuration.setScalesToFit(true);
            configuration.setPixelFormat(NV12_VIDEO_RANGE);
            configuration.setColorMatrix(&*(kCGDisplayStreamYCbCrMatrix_ITU_R_709_2 as *const objc2_core_foundation::CFString));
            configuration.setMinimumFrameInterval(interval(fps));
            configuration.setQueueDepth(QUEUE_DEPTH);
            configuration.setShowsCursor(cursor);

            let shared = Arc::new(Shared::default());
            let output = Output::new(shared.clone());
            let stream = SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter(&content, &display), &configuration, Some(ProtocolObject::from_ref(&*output)));
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(ProtocolObject::from_ref(&*output), SCStreamOutputType::Screen, None)
                .map_err(|error| error.localizedDescription().to_string())?;
            answered(|done| stream.startCaptureWithCompletionHandler(Some(done)))?;

            eprintln!("Sharing a {}×{} screen as {}×{}", monitor.width, monitor.height, size.0, size.1);
            Ok(Self { stream, configuration, _output: output, shared, display: monitor.display, size, left_out })
        })
    }

    /// The size of the pictures `latest` produces.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The newest picture of the screen, a `CVPixelBuffer`, and whether it is one not seen before.
    /// `None` until the first has arrived.
    pub fn latest(&self) -> Option<(Cf, bool)> {
        let mut latest = lock(&self.shared.latest);
        let fresh = std::mem::take(&mut latest.1);
        latest.0.as_ref().map(|picture| (Cf(unsafe { CFRetain(picture.0) }), fresh))
    }

    /// Why macOS stopped the capture, if it has, and whether it was the sharer who asked it to.
    pub fn stopped(&self) -> Option<(String, bool)> {
        lock(&self.shared.stopped).clone()
    }

    fn reconfigure(&self) {
        unsafe { self.stream.updateConfiguration_completionHandler(&self.configuration, None) };
    }

    /// Whether the mouse cursor is drawn into the pictures.
    pub fn show_cursor(&self, visible: bool) {
        unsafe { self.configuration.setShowsCursor(visible) };
        self.reconfigure();
    }

    /// How many pictures a second macOS is asked for at most.
    pub fn set_fps(&self, fps: u32) {
        unsafe { self.configuration.setMinimumFrameInterval(interval(fps)) };
        self.reconfigure();
    }

    /// Looks again at which of the app's overlays are on screen, and leaves the ones there now out
    /// of the picture. The indicator appears when a session starts, which may be after the capture has.
    pub fn leave_out_overlays(&mut self) {
        let now = own_overlays();
        if now == self.left_out {
            return;
        }
        objc2::rc::autoreleasepool(|_| unsafe {
            let Ok(content) = shareable() else { return };
            if let Some(display) = content.displays().iter().find(|display| display.displayID() == self.display) {
                self.stream.updateContentFilter_completionHandler(&filter(&content, &display), None);
                self.left_out = now;
            }
        });
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        // Waited for, so that macOS has stopped calling the output by the time it is let go of.
        let _ = answered(|done| unsafe { self.stream.stopCaptureWithCompletionHandler(Some(done)) });
    }
}
