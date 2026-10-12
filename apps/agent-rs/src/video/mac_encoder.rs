//! H.264 encoding through VideoToolbox, which uses the Mac's own video encoder where it has one.

#![allow(non_upper_case_globals)]

use std::{ffi::c_void, sync::Mutex, time::Duration};

use crate::mac::{boolean, is_true, CFArrayGetCount, CFArrayGetValueAtIndex, CFDictionaryGetValue, CFStringRef, CFTypeRef, Cf};

/// A time as Core Media counts it: `value` ticks of which `timescale` make a second.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct CMTime {
    pub value: i64,
    pub timescale: i32,
    pub flags: u32,
    pub epoch: i64,
}

impl CMTime {
    const INVALID: Self = Self { value: 0, timescale: 0, flags: 0, epoch: 0 };

    fn micros(at: Duration) -> Self {
        Self { value: at.as_micros() as i64, timescale: 1_000_000, flags: 1, epoch: 0 }
    }
}

type Session = *mut c_void;
type SampleBuffer = *const c_void;
type Output = extern "C" fn(*mut c_void, *mut c_void, i32, u32, SampleBuffer);

#[link(name = "VideoToolbox", kind = "framework")]
extern "C" {
    static kVTVideoEncoderSpecification_EnableLowLatencyRateControl: CFStringRef;
    static kVTCompressionPropertyKey_RealTime: CFStringRef;
    static kVTCompressionPropertyKey_ProfileLevel: CFStringRef;
    static kVTCompressionPropertyKey_AllowFrameReordering: CFStringRef;
    static kVTCompressionPropertyKey_AverageBitRate: CFStringRef;
    static kVTCompressionPropertyKey_ExpectedFrameRate: CFStringRef;
    static kVTCompressionPropertyKey_MaxKeyFrameInterval: CFStringRef;
    static kVTCompressionPropertyKey_ColorPrimaries: CFStringRef;
    static kVTCompressionPropertyKey_TransferFunction: CFStringRef;
    static kVTCompressionPropertyKey_YCbCrMatrix: CFStringRef;
    static kVTCompressionPropertyKey_EncoderID: CFStringRef;
    static kVTProfileLevel_H264_Baseline_AutoLevel: CFStringRef;
    static kVTEncodeFrameOptionKey_ForceKeyFrame: CFStringRef;

    fn VTCompressionSessionCreate(
        allocator: CFTypeRef,
        width: i32,
        height: i32,
        codec: u32,
        encoder: CFTypeRef,
        source: CFTypeRef,
        compressed: CFTypeRef,
        output: Output,
        context: *mut c_void,
        session: *mut Session,
    ) -> i32;
    fn VTSessionSetProperty(session: Session, key: CFStringRef, value: CFTypeRef) -> i32;
    fn VTSessionCopyProperty(session: Session, key: CFStringRef, allocator: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn VTCompressionSessionPrepareToEncodeFrames(session: Session) -> i32;
    fn VTCompressionSessionEncodeFrame(session: Session, picture: CFTypeRef, at: CMTime, lasting: CMTime, options: CFTypeRef, context: *mut c_void, flags: *mut u32) -> i32;
    fn VTCompressionSessionCompleteFrames(session: Session, until: CMTime) -> i32;
    fn VTCompressionSessionInvalidate(session: Session);
}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    static kCMSampleAttachmentKey_NotSync: CFStringRef;

    fn CMSampleBufferGetDataBuffer(sample: SampleBuffer) -> CFTypeRef;
    fn CMSampleBufferGetFormatDescription(sample: SampleBuffer) -> CFTypeRef;
    fn CMSampleBufferGetSampleAttachmentsArray(sample: SampleBuffer, create: u8) -> CFTypeRef;
    fn CMBlockBufferGetDataLength(block: CFTypeRef) -> usize;
    fn CMBlockBufferCopyDataBytes(block: CFTypeRef, offset: usize, length: usize, destination: *mut u8) -> i32;
    fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        description: CFTypeRef,
        index: usize,
        set: *mut *const u8,
        size: *mut usize,
        count: *mut usize,
        length_bytes: *mut i32,
    ) -> i32;
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    static kCVImageBufferColorPrimaries_ITU_R_709_2: CFStringRef;
    static kCVImageBufferTransferFunction_ITU_R_709_2: CFStringRef;
    static kCVImageBufferYCbCrMatrix_ITU_R_709_2: CFStringRef;
}

/// `kCMVideoCodecType_H264`
const H264_CODEC: u32 = u32::from_be_bytes(*b"avc1");
const START_CODE: [u8; 4] = [0, 0, 0, 1];
/// Far apart: a keyframe is large, and one is asked for whenever the helper's browser needs it.
const KEYFRAME_EVERY: i64 = 10_000;

/// Where the encoder's thread leaves finished pictures for the capture thread to collect.
type Finished = Mutex<Vec<Vec<u8>>>;

/// An encoded picture as VideoToolbox hands it over, with each unit's length in front of it and the
/// parameter sets kept apart, rewritten as browsers expect it over RTP: every unit after a start code,
/// and the parameter sets in front of each keyframe.
unsafe fn annex_b(sample: SampleBuffer) -> Option<Vec<u8>> {
    let block = CMSampleBufferGetDataBuffer(sample);
    if block.is_null() {
        return None;
    }
    let mut data = vec![0u8; CMBlockBufferGetDataLength(block)];
    if CMBlockBufferCopyDataBytes(block, 0, data.len(), data.as_mut_ptr()) != 0 {
        return None;
    }

    let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, 0);
    let keyframe = attachments.is_null() || CFArrayGetCount(attachments) == 0 || !is_true(CFDictionaryGetValue(CFArrayGetValueAtIndex(attachments, 0), kCMSampleAttachmentKey_NotSync));

    let description = CMSampleBufferGetFormatDescription(sample);
    let (mut sets, mut length_bytes) = (0usize, 4i32);
    CMVideoFormatDescriptionGetH264ParameterSetAtIndex(description, 0, std::ptr::null_mut(), std::ptr::null_mut(), &mut sets, &mut length_bytes);
    let length_bytes = length_bytes.clamp(1, 4) as usize;

    let mut out = Vec::with_capacity(data.len() + 64);
    if keyframe {
        for index in 0..sets {
            let (mut set, mut size) = (std::ptr::null(), 0usize);
            if CMVideoFormatDescriptionGetH264ParameterSetAtIndex(description, index, &mut set, &mut size, std::ptr::null_mut(), std::ptr::null_mut()) == 0 && !set.is_null() {
                out.extend_from_slice(&START_CODE);
                out.extend_from_slice(std::slice::from_raw_parts(set, size));
            }
        }
    }
    let mut rest = data.as_slice();
    while rest.len() > length_bytes {
        let length = rest[..length_bytes].iter().fold(0usize, |length, byte| length << 8 | *byte as usize);
        let Some(unit) = rest.get(length_bytes..length_bytes + length) else { break };
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(unit);
        rest = &rest[length_bytes + length..];
    }
    Some(out)
}

extern "C" fn collect(context: *mut c_void, _frame: *mut c_void, status: i32, _flags: u32, sample: SampleBuffer) {
    if status != 0 || sample.is_null() {
        return;
    }
    // The context is the encoder's `Finished`, which outlives the session that calls this.
    let finished = unsafe { &*(context as *const Finished) };
    if let Some(picture) = unsafe { annex_b(sample) } {
        finished.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(picture);
    }
}

pub struct H264 {
    session: Session,
    finished: Box<Finished>,
    /// What it is, for the helper's stream info.
    pub name: String,
}

impl Drop for H264 {
    fn drop(&mut self) {
        // After this no more pictures are handed over, so `finished` may go.
        unsafe {
            VTCompressionSessionInvalidate(self.session);
            crate::mac::CFRelease(self.session);
        }
    }
}

impl H264 {
    /// `width` and `height` must be even. Pictures are given as the pixel buffers the capture produces.
    pub fn open(width: u32, height: u32, fps: u32, bitrate: u32) -> Result<Self, String> {
        let finished: Box<Finished> = Box::default();
        let context = &*finished as *const Finished as *mut c_void;
        unsafe {
            // Made for calls and screen sharing: no picture is held back to compress it better. Not
            // every Mac's encoder has it, so without it the ordinary rate control is used.
            let low_latency = Cf::dictionary(&[(kVTVideoEncoderSpecification_EnableLowLatencyRateControl, boolean(true))]);
            let mut session: Session = std::ptr::null_mut();
            let mut created = -1;
            for specification in [low_latency.0, std::ptr::null()] {
                created = VTCompressionSessionCreate(std::ptr::null(), width as i32, height as i32, H264_CODEC, specification, std::ptr::null(), std::ptr::null(), collect, context, &mut session);
                if created == 0 && !session.is_null() {
                    break;
                }
            }
            if created != 0 || session.is_null() {
                return Err(format!("no H.264 encoder ({created})"));
            }

            let (rate, bits, far) = (Cf::integer(fps.max(1) as i64), Cf::integer(bitrate as i64), Cf::integer(KEYFRAME_EVERY));
            let properties = [
                (kVTCompressionPropertyKey_RealTime, boolean(true)),
                // What the connection was set up for, and what every browser decodes.
                (kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_Baseline_AutoLevel),
                (kVTCompressionPropertyKey_AllowFrameReordering, boolean(false)),
                (kVTCompressionPropertyKey_AverageBitRate, bits.0),
                (kVTCompressionPropertyKey_ExpectedFrameRate, rate.0),
                (kVTCompressionPropertyKey_MaxKeyFrameInterval, far.0),
                // BT.709, which is what browsers assume for HD video and what the capture produces.
                (kVTCompressionPropertyKey_ColorPrimaries, kCVImageBufferColorPrimaries_ITU_R_709_2),
                (kVTCompressionPropertyKey_TransferFunction, kCVImageBufferTransferFunction_ITU_R_709_2),
                (kVTCompressionPropertyKey_YCbCrMatrix, kCVImageBufferYCbCrMatrix_ITU_R_709_2),
            ];
            for (key, value) in properties {
                // An encoder may not know a property; only the picture's size and codec are essential.
                VTSessionSetProperty(session, key, value);
            }
            VTCompressionSessionPrepareToEncodeFrames(session);

            // Apple's name for the encoder it chose, such as `ave.avc` for the chip's own or `h264.rtvc`
            // for the one made for calls, which uses the chip too.
            let mut id: CFTypeRef = std::ptr::null();
            VTSessionCopyProperty(session, kVTCompressionPropertyKey_EncoderID, std::ptr::null(), &mut id);
            let id = Cf::owned(id).and_then(|id| crate::mac::text(id.0)).unwrap_or_default();
            let name = format!("VideoToolbox {}", id.trim_start_matches("com.apple.videotoolbox.videoencoder.")).trim_end().to_owned();
            Ok(Self { session, finished, name })
        }
    }

    pub fn set_bitrate(&mut self, bitrate: u32) {
        let bits = Cf::integer(bitrate as i64);
        unsafe { VTSessionSetProperty(self.session, kVTCompressionPropertyKey_AverageBitRate, bits.0) };
    }

    /// Encodes one picture, a `CVPixelBuffer`, shown `at` this long after the stream began, and passes
    /// what comes out to `sink` in Annex B form.
    pub fn encode(&mut self, picture: CFTypeRef, at: Duration, keyframe: bool, mut sink: impl FnMut(Vec<u8>)) -> Result<(), String> {
        unsafe {
            let options = keyframe.then(|| Cf::dictionary(&[(kVTEncodeFrameOptionKey_ForceKeyFrame, boolean(true))]));
            let options = options.as_ref().map_or(std::ptr::null(), |options| options.0);
            let status = VTCompressionSessionEncodeFrame(self.session, picture, CMTime::micros(at), CMTime::INVALID, options, std::ptr::null_mut(), std::ptr::null_mut());
            if status != 0 {
                return Err(format!("the encoder refused a picture ({status})"));
            }
            // Waits for the picture to come out, a few milliseconds, so that it is sent at once.
            VTCompressionSessionCompleteFrames(self.session, CMTime::INVALID);
        }
        for picture in std::mem::take(&mut *self.finished.lock().unwrap_or_else(|poisoned| poisoned.into_inner())) {
            sink(picture);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[link(name = "CoreVideo", kind = "framework")]
    extern "C" {
        fn CVPixelBufferCreate(allocator: CFTypeRef, width: usize, height: usize, format: u32, attributes: CFTypeRef, buffer: *mut CFTypeRef) -> i32;
        fn CVPixelBufferLockBaseAddress(buffer: CFTypeRef, flags: u64) -> i32;
        fn CVPixelBufferUnlockBaseAddress(buffer: CFTypeRef, flags: u64) -> i32;
        fn CVPixelBufferGetBaseAddressOfPlane(buffer: CFTypeRef, plane: usize) -> *mut u8;
        fn CVPixelBufferGetBytesPerRowOfPlane(buffer: CFTypeRef, plane: usize) -> usize;
    }

    /// The units of an Annex B stream, by type.
    fn unit_types(stream: &[u8]) -> Vec<u8> {
        stream.windows(5).filter(|bytes| bytes[..4] == START_CODE).map(|bytes| bytes[4] & 0x1f).collect()
    }

    /// A gray picture with a square that moves, so that there is something to encode.
    unsafe fn picture(width: usize, height: usize, step: usize) -> Cf {
        let mut buffer = std::ptr::null();
        let surface = Cf::dictionary(&[]);
        let attributes = Cf::dictionary(&[(Cf::string("IOSurfaceProperties").0, surface.0)]);
        assert_eq!(CVPixelBufferCreate(std::ptr::null(), width, height, u32::from_be_bytes(*b"420v"), attributes.0, &mut buffer), 0);
        CVPixelBufferLockBaseAddress(buffer, 0);
        let (luma, row) = (CVPixelBufferGetBaseAddressOfPlane(buffer, 0), CVPixelBufferGetBytesPerRowOfPlane(buffer, 0));
        for y in 0..height {
            for x in 0..width {
                let inside = (x.wrapping_sub(step * 8)) < 64 && y > 100 && y < 164;
                *luma.add(y * row + x) = if inside { 235 } else { 90 };
            }
        }
        let (chroma, row) = (CVPixelBufferGetBaseAddressOfPlane(buffer, 1), CVPixelBufferGetBytesPerRowOfPlane(buffer, 1));
        std::slice::from_raw_parts_mut(chroma, row * height / 2).fill(128);
        CVPixelBufferUnlockBaseAddress(buffer, 0);
        Cf(buffer)
    }

    #[test]
    fn pictures_come_out_as_a_stream_a_browser_can_start_from() {
        let mut encoder = H264::open(640, 360, 30, 1_000_000).expect("this Mac has an H.264 encoder");
        let mut pictures = Vec::new();
        for step in 0..30 {
            let frame = unsafe { picture(640, 360, step) };
            let at = Duration::from_millis(step as u64 * 33);
            // A keyframe to begin with, and one asked for half way.
            encoder.encode(frame.0, at, step == 15, |data| pictures.push(data)).expect("the picture is encoded");
        }
        assert_eq!(pictures.len(), 30, "one picture out for each one in, with none held back");

        // Sequence and picture parameter sets (7, 8), then the keyframe itself (5).
        let first = unit_types(&pictures[0]);
        assert!(first.starts_with(&[7, 8]) && first.contains(&5), "the stream starts with {first:?}");
        assert!(unit_types(&pictures[1]).contains(&1) && !unit_types(&pictures[1]).contains(&7), "an ordinary picture follows");
        assert!(unit_types(&pictures[15]).starts_with(&[7, 8]), "a keyframe comes when asked for");
        // Baseline: the profile in the sequence parameter set is 66.
        assert_eq!(pictures[0][5], 66);
        eprintln!("encoded by {}, {} bytes for a second of video", encoder.name, pictures.iter().map(Vec::len).sum::<usize>());
    }
}
