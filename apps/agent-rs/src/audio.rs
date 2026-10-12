//! The sound the sharer's computer plays, for the helper to hear along with the screen.
//!
//! Everything the computer plays is captured, with one exception: the program that is using the
//! microphone, and whatever it started. That is nearly always the call the two people are talking
//! on, and a helper who heard it would hear the sharer's side of the conversation twice and their
//! own voice a moment late.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::sync::mpsc;

pub const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// Opus frames of 20 ms, the length browsers expect.
const FRAME_SAMPLES: usize = 960;
const BITRATE_BPS: i32 = 96_000;
/// How often to look again for which program is using the microphone.
const CALL_CHECK_EVERY: Duration = Duration::from_secs(3);

/// One Opus frame and where it starts, counted in samples since capturing began.
pub struct AudioFrame {
    pub data: Vec<u8>,
    pub at_sample: u64,
}

/// Captures for as long as it lives.
pub struct AudioSource {
    frames: mpsc::Receiver<AudioFrame>,
    stop: Arc<AtomicBool>,
}

impl AudioSource {
    pub fn system() -> Self {
        let (tx, frames) = mpsc::channel(50);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            // A computer with no sound, or a Windows too old to leave one program out, shares none.
            if let Err(error) = run(tx, &stopped) {
                eprintln!("The computer's sound is not shared: {error}");
            }
        });
        Self { frames, stop }
    }

    pub async fn next(&mut self) -> Option<AudioFrame> {
        self.frames.recv().await
    }
}

impl Drop for AudioSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Turns 20 ms of 16-bit stereo into one Opus frame.
struct Opus(*mut unsafe_libopus::OpusEncoder);

impl Opus {
    fn new() -> Result<Self, String> {
        let mut error = 0;
        let encoder = unsafe { unsafe_libopus::opus_encoder_create(SAMPLE_RATE as i32, CHANNELS as i32, unsafe_libopus::OPUS_APPLICATION_AUDIO, &mut error) };
        if encoder.is_null() || error != 0 {
            return Err(format!("the Opus encoder did not start ({error})"));
        }
        unsafe { unsafe_libopus::opus_encoder_ctl!(encoder, unsafe_libopus::OPUS_SET_BITRATE_REQUEST, BITRATE_BPS) };
        Ok(Self(encoder))
    }

    fn encode(&mut self, samples: &[i16]) -> Option<Vec<u8>> {
        let mut data = vec![0u8; 1500];
        let length = unsafe { unsafe_libopus::opus_encode(self.0, samples.as_ptr(), FRAME_SAMPLES as i32, data.as_mut_ptr(), data.len() as i32) };
        (length > 0).then(|| {
            data.truncate(length as usize);
            data
        })
    }
}

impl Drop for Opus {
    fn drop(&mut self) {
        unsafe { unsafe_libopus::opus_encoder_destroy(self.0) };
    }
}

#[cfg(windows)]
fn run(tx: mpsc::Sender<AudioFrame>, stopped: &AtomicBool) -> Result<(), String> {
    use std::time::Instant;

    use windows::{
        core::{implement, Interface, Ref, GUID, HRESULT, PCWSTR},
        Win32::{
            Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
            Media::Audio::*,
            System::{
                Com::{CoCreateInstance, CLSCTX_ALL},
                Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
                Threading::{CreateEventW, WaitForSingleObject},
            },
        },
    };

    #[implement(IActivateAudioInterfaceCompletionHandler)]
    struct Activated(std::sync::mpsc::SyncSender<()>);

    impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
        fn ActivateCompleted(&self, _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
            let _ = self.0.try_send(());
            Ok(())
        }
    }

    /// The shape Windows expects the activation parameters in: a `PROPVARIANT` holding a blob.
    #[repr(C)]
    struct BlobVariant {
        kind: u16,
        reserved: [u16; 3],
        size: u32,
        data: *const AUDIOCLIENT_ACTIVATION_PARAMS,
    }
    const VT_BLOB: u16 = 65;

    /// Every running process with the process that started it.
    unsafe fn parents() -> Vec<(u32, u32, String)> {
        let mut found = Vec::new();
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return found };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let length = entry.szExeFile.iter().position(|&unit| unit == 0).unwrap_or(entry.szExeFile.len());
            found.push((entry.th32ProcessID, entry.th32ParentProcessID, String::from_utf16_lossy(&entry.szExeFile[..length]).to_lowercase()));
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }

    /// The program using a microphone, if one is: the call to leave out. A program made of several
    /// processes records in one and plays in another, so it is the one they all descend from that counts.
    unsafe fn call_program() -> Option<u32> {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let microphones = devices.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE).ok()?;
        let mut recording = None;
        'devices: for index in 0..microphones.GetCount().ok()? {
            let Ok(manager) = microphones.Item(index).and_then(|device| device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)) else { continue };
            let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
            for session in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(control) = sessions.GetSession(session).and_then(|control| control.cast::<IAudioSessionControl2>()) else { continue };
                let process = control.GetProcessId().unwrap_or(0);
                if control.GetState() == Ok(AudioSessionStateActive) && process != 0 && process != std::process::id() {
                    recording = Some(process);
                    break 'devices;
                }
            }
        }
        let mut process = recording?;

        let all = parents();
        let name = |id: u32| all.iter().find(|(process, _, _)| *process == id).map(|(_, _, name)| name.as_str());
        // Up to the first ancestor that is the same program; the shell and the system start everything
        // and are nobody's "program".
        let own = name(process)?.to_owned();
        while let Some(&(_, parent, _)) = all.iter().find(|(id, _, _)| *id == process) {
            match name(parent) {
                Some(parent_name) if parent != 0 && parent != process && (parent_name == own || !["explorer.exe", "svchost.exe", "services.exe", "sihost.exe", "wininit.exe", "winlogon.exe", "system"].contains(&parent_name)) => process = parent,
                _ => break,
            }
        }
        Some(process)
    }

    /// Opens a capture of everything this computer plays, except what `left_out` and the processes
    /// it started play.
    unsafe fn open(left_out: u32) -> Result<(IAudioClient, IAudioCaptureClient, HANDLE), String> {
        let text = |error: windows::core::Error| error.message();
        let parameters = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: left_out,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        let variant = BlobVariant { kind: VT_BLOB, reserved: [0; 3], size: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, data: &parameters };

        let (done, completed) = std::sync::mpsc::sync_channel(1);
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(done).into();
        let operation = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID as *const GUID,
            Some(&variant as *const BlobVariant as *const _),
            &handler,
        )
        .map_err(text)?;
        completed.recv_timeout(Duration::from_secs(5)).map_err(|_| "Windows did not answer".to_owned())?;
        let (mut outcome, mut activated) = (HRESULT(0), None);
        operation.GetActivateResult(&mut outcome, &mut activated).map_err(text)?;
        outcome.ok().map_err(text)?;
        let client: IAudioClient = activated.ok_or("no audio client")?.cast().map_err(text)?;

        let block = (CHANNELS * 2) as u16;
        let format = WAVEFORMATEX {
            wFormatTag: 1, // PCM
            nChannels: CHANNELS as u16,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * block as u32,
            nBlockAlign: block,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                200_000, // 20 ms, in units of 100 ns
                0,
                &format,
                None,
            )
            .map_err(text)?;
        let ready = CreateEventW(None, false, false, PCWSTR::null()).map_err(text)?;
        client.SetEventHandle(ready).map_err(text)?;
        let capture: IAudioCaptureClient = client.GetService().map_err(text)?;
        client.Start().map_err(text)?;
        Ok((client, capture, ready))
    }

    unsafe {
        let mut encoder = Opus::new()?;
        // Samples waiting to fill a frame, and how many have been captured in all.
        let mut waiting: Vec<i16> = Vec::with_capacity(FRAME_SAMPLES * CHANNELS * 4);
        let mut captured: u64 = 0;
        let began = Instant::now();

        loop {
            // With nobody on a call, the agent leaves out itself, which plays nothing.
            let left_out = call_program();
            let (client, capture, ready) = open(left_out.unwrap_or(std::process::id()))?;
            let mut checked = Instant::now();

            let again = loop {
                if stopped.load(Ordering::Relaxed) {
                    break false;
                }
                if checked.elapsed() >= CALL_CHECK_EVERY {
                    checked = Instant::now();
                    if call_program() != left_out {
                        break true;
                    }
                }
                if WaitForSingleObject(ready, 100) != WAIT_OBJECT_0 {
                    continue;
                }
                while capture.GetNextPacketSize().unwrap_or(0) > 0 {
                    let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
                    if capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
                        break;
                    }
                    let count = frames as usize * CHANNELS;
                    if waiting.is_empty() {
                        // Nothing plays while the computer is silent, so the clock, not the count of
                        // samples, says where sound that starts again belongs.
                        captured = captured.max(began.elapsed().as_micros() as u64 * SAMPLE_RATE as u64 / 1_000_000);
                    }
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                        waiting.resize(waiting.len() + count, 0);
                    } else {
                        waiting.extend_from_slice(std::slice::from_raw_parts(data as *const i16, count));
                    }
                    let _ = capture.ReleaseBuffer(frames);

                    while waiting.len() >= FRAME_SAMPLES * CHANNELS {
                        let frame: Vec<i16> = waiting.drain(..FRAME_SAMPLES * CHANNELS).collect();
                        if let Some(data) = encoder.encode(&frame) {
                            match tx.try_send(AudioFrame { data, at_sample: captured }) {
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    let _ = client.Stop();
                                    let _ = CloseHandle(ready);
                                    return Ok(());
                                }
                                // A frame the connection has no room for is simply not heard.
                                _ => {}
                            }
                        }
                        captured += FRAME_SAMPLES as u64;
                    }
                }
            };
            let _ = client.Stop();
            let _ = CloseHandle(ready);
            if !again {
                return Ok(());
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn run(tx: mpsc::Sender<AudioFrame>, stopped: &AtomicBool) -> Result<(), String> {
    use std::time::Instant;

    // With nobody on a call, nothing is left out but the agent itself, which plays nothing.
    let mut left_out = mac::call_program();
    let (capture, samples) = mac::Capture::open(&left_out)?;
    let mut checked = Instant::now();

    let mut encoder = Opus::new()?;
    // Samples waiting to fill a frame, and how many have been captured in all.
    let mut waiting: Vec<i16> = Vec::with_capacity(FRAME_SAMPLES * CHANNELS * 4);
    let mut captured: u64 = 0;
    let began = Instant::now();

    while !stopped.load(Ordering::Relaxed) {
        if checked.elapsed() >= CALL_CHECK_EVERY {
            checked = Instant::now();
            let now = mac::call_program();
            if now != left_out {
                capture.leave_out(&now);
                left_out = now;
            }
        }
        let Ok(heard) = samples.recv_timeout(Duration::from_millis(100)) else { continue };
        if waiting.is_empty() {
            // The clock, not the count of samples, says where sound that starts again belongs.
            captured = captured.max(began.elapsed().as_micros() as u64 * SAMPLE_RATE as u64 / 1_000_000);
        }
        waiting.extend_from_slice(&heard);

        while waiting.len() >= FRAME_SAMPLES * CHANNELS {
            let frame: Vec<i16> = waiting.drain(..FRAME_SAMPLES * CHANNELS).collect();
            if let Some(data) = encoder.encode(&frame) {
                // A frame the connection has no room for is simply not heard.
                if let Err(mpsc::error::TrySendError::Closed(_)) = tx.try_send(AudioFrame { data, at_sample: captured }) {
                    return Ok(());
                }
            }
            captured += FRAME_SAMPLES as u64;
        }
    }
    Ok(())
}

/// macOS's side of it: what the computer plays, from ScreenCaptureKit, which gives it along with
/// the screen and under the same permission.
#[cfg(target_os = "macos")]
mod mac {
    use std::{ffi::c_void, sync::mpsc, time::Duration};

    use block2::RcBlock;
    use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, DefinedClass};
    use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
    use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCDisplay, SCRunningApplication, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput, SCStreamOutputType, SCWindow,
    };

    use super::{CHANNELS, SAMPLE_RATE};
    use crate::mac::{may_record_screen, text, CFRelease, CFTypeRef, Cf};

    #[repr(C)]
    struct AudioBuffer {
        channels: u32,
        bytes: u32,
        data: *const c_void,
    }

    /// An `AudioBufferList` with room for a buffer for each channel.
    #[repr(C)]
    struct AudioBuffers {
        count: u32,
        buffers: [AudioBuffer; CHANNELS],
    }

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreMedia", kind = "framework")]
    extern "C" {
        fn CMSampleBufferGetNumSamples(sample: *const c_void) -> isize;
        fn CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sample: *const c_void,
            needed: *mut usize,
            list: *mut AudioBuffers,
            size: usize,
            structure_allocator: CFTypeRef,
            block_allocator: CFTypeRef,
            flags: u32,
            block: *mut CFTypeRef,
        ) -> i32;
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(object: u32, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32) -> i32;
        fn AudioObjectGetPropertyData(object: u32, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32, data: *mut c_void) -> i32;
    }

    const fn code(name: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*name)
    }
    /// `kAudioObjectSystemObject`
    const SYSTEM: u32 = 1;
    const GLOBAL: u32 = code(b"glob");
    /// The processes using sound, whether each is recording, and what each is: properties that
    /// macOS has from version 14.2.
    const PROCESSES: u32 = code(b"prs#");
    const IS_RECORDING: u32 = code(b"piri");
    const PROCESS_ID: u32 = code(b"ppid");
    const BUNDLE_ID: u32 = code(b"pbid");

    const ANSWER_WITHIN: Duration = Duration::from_secs(5);

    unsafe fn property<T: Copy>(object: u32, selector: u32, start: T) -> Option<T> {
        let address = Address { selector, scope: GLOBAL, element: 0 };
        let (mut value, mut size) = (start, size_of::<T>() as u32);
        (AudioObjectGetPropertyData(object, &address, 0, std::ptr::null(), &mut size, (&mut value as *mut T).cast()) == 0).then_some(value)
    }

    /// The program using a microphone, if one is: the call to leave out. It is named by its process
    /// and by its bundle, because a browser records in one process and is known to macOS as another.
    pub fn call_program() -> Option<(i32, String)> {
        unsafe {
            let address = Address { selector: PROCESSES, scope: GLOBAL, element: 0 };
            let mut size = 0u32;
            if AudioObjectGetPropertyDataSize(SYSTEM, &address, 0, std::ptr::null(), &mut size) != 0 {
                return None;
            }
            let mut processes = vec![0u32; size as usize / 4];
            if AudioObjectGetPropertyData(SYSTEM, &address, 0, std::ptr::null(), &mut size, processes.as_mut_ptr().cast()) != 0 {
                return None;
            }
            processes.truncate(size as usize / 4);
            processes.into_iter().find_map(|process| {
                let recording = property(process, IS_RECORDING, 0u32)? != 0;
                let id = property(process, PROCESS_ID, 0i32)?;
                if !recording || id == std::process::id() as i32 {
                    return None;
                }
                let bundle = property::<CFTypeRef>(process, BUNDLE_ID, std::ptr::null()).and_then(Cf::owned).and_then(|bundle| text(bundle.0));
                Some((id, bundle.unwrap_or_default()))
            })
        }
    }

    define_class!(
        // SAFETY: `NSObject` has no rules for its subclasses, and this one has no `Drop`.
        #[unsafe(super(NSObject))]
        #[name = "LightAssistSoundOutput"]
        #[ivars = mpsc::SyncSender<Vec<i16>>]
        struct Output;

        unsafe impl NSObjectProtocol for Output {}

        unsafe impl SCStreamOutput for Output {
            #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
            unsafe fn stream_did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
                if kind != SCStreamOutputType::Audio {
                    return;
                }
                if let Some(heard) = interleaved(sample as *const CMSampleBuffer as *const c_void) {
                    // Sound nobody is taking is simply not heard.
                    let _ = self.ivars().try_send(heard);
                }
            }
        }
    );

    /// The samples in a buffer of sound, as 16-bit stereo. macOS gives them as numbers between -1
    /// and 1, each channel in a buffer of its own.
    unsafe fn interleaved(sample: *const c_void) -> Option<Vec<i16>> {
        let frames = CMSampleBufferGetNumSamples(sample) as usize;
        let mut list = std::mem::zeroed::<AudioBuffers>();
        let mut block: CFTypeRef = std::ptr::null();
        let read = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, std::ptr::null_mut(), &mut list, size_of::<AudioBuffers>(), std::ptr::null(), std::ptr::null(), 0, &mut block);
        if read != 0 || block.is_null() {
            return None;
        }
        let whole = |value: f32| (value.clamp(-1.0, 1.0) * 32767.0) as i16;
        let channel = |buffer: &AudioBuffer| std::slice::from_raw_parts(buffer.data as *const f32, buffer.bytes as usize / 4);
        let mut heard = Vec::with_capacity(frames * CHANNELS);
        match (list.count as usize, list.buffers[0].channels as usize) {
            (CHANNELS, 1) => {
                let (left, right) = (channel(&list.buffers[0]), channel(&list.buffers[1]));
                for (left, right) in left.iter().zip(right).take(frames) {
                    heard.extend([whole(*left), whole(*right)]);
                }
            }
            (1, CHANNELS) => heard.extend(channel(&list.buffers[0]).iter().take(frames * CHANNELS).map(|value| whole(*value))),
            _ => {}
        }
        CFRelease(block);
        (!heard.is_empty()).then_some(heard)
    }

    struct Carried<T>(T);
    unsafe impl<T> Send for Carried<T> {}

    fn shareable() -> Result<Retained<SCShareableContent>, String> {
        let (done, answer) = mpsc::sync_channel(1);
        let block = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
            let content = unsafe { Retained::retain(content) }.map(Carried);
            let _ = done.try_send(content.ok_or_else(|| unsafe { error.as_ref() }.map_or("nothing to capture".to_owned(), |error| error.localizedDescription().to_string())));
        });
        unsafe { SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(false, true, &block) };
        answer.recv_timeout(ANSWER_WITHIN).map_err(|_| "macOS did not answer".to_owned())?.map(|content| content.0)
    }

    /// Everything the computer plays, except what the call's program plays.
    unsafe fn filter(left_out: &Option<(i32, String)>) -> Result<Retained<SCContentFilter>, String> {
        let content = shareable()?;
        let display: Retained<SCDisplay> = content.displays().iter().next().ok_or("no display")?;
        let call: Vec<Retained<SCRunningApplication>> = match left_out {
            Some((process, bundle)) => content
                .applications()
                .iter()
                .filter(|app| {
                    let known_as = app.bundleIdentifier().to_string();
                    app.processID() == *process || (!known_as.is_empty() && bundle.starts_with(&known_as))
                })
                .collect(),
            None => Vec::new(),
        };
        Ok(SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            &display,
            &NSArray::from_retained_slice(&call),
            &NSArray::<SCWindow>::new(),
        ))
    }

    /// Captures for as long as it lives.
    pub struct Capture {
        stream: Retained<SCStream>,
        _output: Retained<Output>,
    }

    impl Capture {
        /// Starts capturing, and returns where the sound arrives: 16-bit stereo at 48 kHz.
        pub fn open(left_out: &Option<(i32, String)>) -> Result<(Self, mpsc::Receiver<Vec<i16>>), String> {
            if !may_record_screen() {
                return Err("macOS has not been told to let Light Assist record the screen, which sound comes with".to_owned());
            }
            objc2::rc::autoreleasepool(|_| unsafe {
                let configuration = SCStreamConfiguration::new();
                configuration.setCapturesAudio(true);
                configuration.setSampleRate(SAMPLE_RATE as isize);
                configuration.setChannelCount(CHANNELS as isize);
                configuration.setExcludesCurrentProcessAudio(true);
                // A stream always has a picture; this one's is as small and as rare as it can be.
                configuration.setWidth(2);
                configuration.setHeight(2);
                configuration.setMinimumFrameInterval(CMTime { value: 1, timescale: 1, flags: CMTimeFlags::Valid, epoch: 0 });

                // About a second of sound may wait, in the pieces macOS delivers it in.
                let (heard, samples) = mpsc::sync_channel(50);
                let output: Retained<Output> = msg_send![super(Output::alloc().set_ivars(heard)), init];
                let stream = SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &*filter(left_out)?, &configuration, None);
                // Taking the picture as well keeps macOS from complaining that nobody does.
                for kind in [SCStreamOutputType::Audio, SCStreamOutputType::Screen] {
                    stream
                        .addStreamOutput_type_sampleHandlerQueue_error(ProtocolObject::from_ref(&*output), kind, None)
                        .map_err(|error| error.localizedDescription().to_string())?;
                }

                let (done, answer) = mpsc::sync_channel(1);
                let started = RcBlock::new(move |error: *mut NSError| {
                    let _ = done.try_send(error.as_ref().map(|error| error.localizedDescription().to_string()));
                });
                stream.startCaptureWithCompletionHandler(Some(&started));
                match answer.recv_timeout(ANSWER_WITHIN) {
                    Ok(None) => Ok((Self { stream, _output: output }, samples)),
                    Ok(Some(error)) => Err(error),
                    Err(_) => Err("macOS did not answer".to_owned()),
                }
            })
        }

        /// Leaves another program out from now on, or none.
        pub fn leave_out(&self, left_out: &Option<(i32, String)>) {
            objc2::rc::autoreleasepool(|_| unsafe {
                if let Ok(filter) = filter(left_out) {
                    self.stream.updateContentFilter_completionHandler(&filter, None);
                }
            });
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            // Waited for, so that macOS has stopped calling the output by the time it is let go of.
            let (done, answer) = mpsc::sync_channel(1);
            let stopped = RcBlock::new(move |_error: *mut NSError| {
                let _ = done.try_send(());
            });
            unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&stopped)) };
            let _ = answer.recv_timeout(ANSWER_WITHIN);
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run(_tx: mpsc::Sender<AudioFrame>, _stopped: &AtomicBool) -> Result<(), String> {
    let _ = (Opus::new, CALL_CHECK_EVERY, FRAME_SAMPLES);
    Err("sharing sound is not implemented on this platform".to_owned())
}
