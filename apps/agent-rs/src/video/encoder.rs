//! H.264 encoding through Media Foundation.

use std::{
    mem::ManuallyDrop,
    time::{Duration, Instant},
};

use windows::{
    core::{Error, Interface, Result, GUID, PWSTR},
    Win32::{
        Foundation::{E_FAIL, VARIANT_BOOL, VARIANT_TRUE},
        Media::MediaFoundation::*,
        System::{
            Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER},
            Variant::{VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4},
        },
    },
};

fn variant(vt: VARENUM, value: VARIANT_0_0_0) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 { vt, wReserved1: 0, wReserved2: 0, wReserved3: 0, Anonymous: value }),
        },
    }
}

fn number(value: u32) -> VARIANT {
    variant(VT_UI4, VARIANT_0_0_0 { ulVal: value })
}

fn flag(value: bool) -> VARIANT {
    variant(VT_BOOL, VARIANT_0_0_0 { boolVal: if value { VARIANT_TRUE } else { VARIANT_BOOL(0) } })
}

fn pair(high: u32, low: u32) -> u64 {
    (high as u64) << 32 | low as u64
}

fn has_parameter_sets(annex_b: &[u8]) -> bool {
    // NAL unit type 7 is a sequence parameter set.
    annex_b.windows(4).any(|bytes| bytes[..3] == [0, 0, 1] && bytes[3] & 0x1f == 7)
}

/// How long to wait for a hardware encoder to ask for a picture before giving up on it.
const INPUT_TIMEOUT: Duration = Duration::from_secs(2);
/// How long to wait for a picture to come out of a hardware encoder before sending what there is. In
/// low-latency mode it is usually a few milliseconds; anything later goes out with the next picture.
const OUTPUT_WAIT: Duration = Duration::from_millis(25);

/// An H.264 encoder: the graphics card's when there is one that works, otherwise Microsoft's software
/// encoder, which is present on every Windows install.
pub struct H264 {
    transform: IMFTransform,
    codec: ICodecAPI,
    /// What it is, as Windows names it, for the helper's stream info.
    pub name: String,
    /// A hardware encoder works asynchronously: it says, with events, when it wants a picture and when
    /// it has one ready. The software encoder simply takes a picture and gives one back.
    events: Option<IMFMediaEventGenerator>,
    /// Pictures the hardware encoder has asked for and not yet been given.
    wanted: u32,
    /// Where a hardware encoder came from; it is shut down through this.
    activate: Option<IMFActivate>,
    /// Media Foundation counts time in 100-nanosecond units.
    frame_duration: i64,
    frame_bytes: u32,
}

impl Drop for H264 {
    fn drop(&mut self) {
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            if let Some(activate) = &self.activate {
                let _ = activate.ShutdownObject();
            }
        }
    }
}

unsafe fn friendly_name(activate: &IMFActivate) -> String {
    let (mut text, mut length) = (PWSTR::null(), 0);
    if activate.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut text, &mut length).is_err() {
        return "a hardware encoder".to_owned();
    }
    let name = text.to_string().unwrap_or_default();
    CoTaskMemFree(Some(text.0 as *const _));
    name
}

/// The graphics card's H.264 encoders that take NV12, best first as Windows ranks them.
unsafe fn hardware_encoders() -> Vec<IMFActivate> {
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
    let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
    let (mut list, mut count) = (std::ptr::null_mut(), 0u32);
    let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
    if MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &mut list, &mut count).is_err() || list.is_null() {
        return Vec::new();
    }
    let found = (0..count as usize).filter_map(|index| (*list.add(index)).take()).collect();
    CoTaskMemFree(Some(list as *const _));
    found
}

impl H264 {
    /// `width` and `height` must be even. Input is NV12 in BT.709 with limited range.
    ///
    /// `LIGHT_ASSIST_ENCODER=software` skips the graphics card, to compare the two.
    pub fn open(width: u32, height: u32, fps: u32, bitrate: u32) -> Result<Self> {
        unsafe {
            if std::env::var("LIGHT_ASSIST_ENCODER").as_deref() != Ok("software") {
                for activate in hardware_encoders() {
                    let name = friendly_name(&activate);
                    let configured = activate
                        .ActivateObject::<IMFTransform>()
                        .and_then(|transform| Self::configure(transform, Some(activate.clone()), &name, width, height, fps, bitrate));
                    match configured {
                        Ok(encoder) => return Ok(encoder),
                        Err(error) => {
                            eprintln!("{name} cannot be used, trying the next encoder: {error}");
                            let _ = activate.ShutdownObject();
                        }
                    }
                }
            }
            let transform: IMFTransform = CoCreateInstance(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER)?;
            Self::configure(transform, None, "Microsoft's software encoder", width, height, fps, bitrate)
        }
    }

    unsafe fn configure(
        transform: IMFTransform,
        activate: Option<IMFActivate>,
        name: &str,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
    ) -> Result<Self> {
        let video_type = |subtype: &GUID| -> Result<IMFMediaType> {
            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
            media_type.SetUINT64(&MF_MT_FRAME_SIZE, pair(width, height))?;
            media_type.SetUINT64(&MF_MT_FRAME_RATE, pair(fps, 1))?;
            media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pair(1, 1))?;
            media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            media_type.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
            media_type.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
            Ok(media_type)
        };

        let attributes = transform.GetAttributes().ok();
        let asynchronous = attributes.as_ref().is_some_and(|attributes| attributes.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0);
        if let Some(attributes) = &attributes {
            // An asynchronous encoder refuses to do anything until told the caller knows it is one.
            if asynchronous {
                attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
            }
            let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
        }

        let codec: ICodecAPI = transform.cast()?;
        // Encoders differ in what they accept; each setting is a request. Rate control has to be chosen
        // before the output type is set.
        let _ = codec.SetValue(&CODECAPI_AVEncCommonRateControlMode, &number(eAVEncCommonRateControlMode_CBR.0 as u32));
        let _ = codec.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &number(bitrate));
        // Low-latency mode stops the encoder holding frames back and disables B-frames.
        let _ = codec.SetValue(&CODECAPI_AVLowLatencyMode, &flag(true));
        // Keyframes are sent on request, so regular ones can be far apart.
        let _ = codec.SetValue(&CODECAPI_AVEncMPVGOPSize, &number(fps * 10));
        let _ = codec.SetValue(&CODECAPI_AVEncMPVDefaultBPictureCount, &number(0));

        // Baseline is what the browser was offered first; browsers decode the others as well, so they
        // are fallbacks for encoders that do not do baseline.
        let mut accepted = Err(Error::from(E_FAIL));
        for profile in [eAVEncH264VProfile_ConstrainedBase, eAVEncH264VProfile_Base, eAVEncH264VProfile_Main, eAVEncH264VProfile_High] {
            let output = video_type(&MFVideoFormat_H264)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, profile.0 as u32)?;
            accepted = transform.SetOutputType(0, &output, 0);
            if accepted.is_ok() {
                break;
            }
        }
        accepted?;
        transform.SetInputType(0, &video_type(&MFVideoFormat_NV12)?, 0)?;

        transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
        transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;

        let events = if asynchronous { Some(transform.cast::<IMFMediaEventGenerator>()?) } else { None };
        eprintln!("Encoding with {name}");
        Ok(Self { transform, codec, name: name.to_owned(), events, wanted: 0, activate, frame_duration: 10_000_000 / fps as i64, frame_bytes: width * height * 3 / 2 })
    }

    /// Changes the bitrate without starting again; encoders take it from the next pictures on.
    pub fn set_bitrate(&self, bitrate: u32) {
        unsafe {
            let _ = self.codec.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &number(bitrate));
        }
    }

    /// Encodes one picture and hands every finished frame, in Annex B form, to `emit`.
    pub fn encode(&mut self, nv12: &[u8], frame: u64, keyframe: bool, mut emit: impl FnMut(Vec<u8>)) -> Result<()> {
        unsafe {
            if keyframe {
                let _ = self.codec.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &number(1));
            }

            let buffer = MFCreateMemoryBuffer(self.frame_bytes)?;
            let mut target = std::ptr::null_mut();
            buffer.Lock(&mut target, None, None)?;
            std::ptr::copy_nonoverlapping(nv12.as_ptr(), target, nv12.len().min(self.frame_bytes as usize));
            buffer.Unlock()?;
            buffer.SetCurrentLength(self.frame_bytes)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(frame as i64 * self.frame_duration)?;
            sample.SetSampleDuration(self.frame_duration)?;

            if self.events.is_none() {
                self.transform.ProcessInput(0, &sample, 0)?;
                while let Some(data) = self.take_output()? {
                    emit(data);
                }
                return Ok(());
            }

            // Asynchronous: give the picture only when asked for one, then wait briefly for it to come out.
            let asked_by = Instant::now() + INPUT_TIMEOUT;
            while self.wanted == 0 {
                if self.handle_events(&mut emit)? == 0 {
                    if Instant::now() > asked_by {
                        return Err(Error::new(E_FAIL, "the encoder stopped asking for pictures"));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            self.transform.ProcessInput(0, &sample, 0)?;
            self.wanted -= 1;

            let ready_by = Instant::now() + OUTPUT_WAIT;
            let emitted = std::cell::Cell::new(0);
            let mut counting = |data| {
                emitted.set(emitted.get() + 1);
                emit(data)
            };
            loop {
                let handled = self.handle_events(&mut counting)?;
                if emitted.get() > 0 || Instant::now() >= ready_by {
                    break;
                }
                if handled == 0 {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            Ok(())
        }
    }

    /// Deals with every event the hardware encoder has waiting, and returns how many there were.
    unsafe fn handle_events(&mut self, emit: &mut impl FnMut(Vec<u8>)) -> Result<u32> {
        let Some(events) = self.events.clone() else { return Ok(0) };
        let mut handled = 0;
        loop {
            let event = match events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                Ok(event) => event,
                Err(error) if error.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(handled),
                Err(error) => return Err(error),
            };
            handled += 1;
            let kind = MF_EVENT_TYPE(event.GetType()? as i32);
            if kind == METransformNeedInput {
                self.wanted += 1;
            } else if kind == METransformHaveOutput {
                if let Some(data) = self.take_output()? {
                    emit(data);
                }
            }
        }
    }

    /// The parameter sets a decoder needs before the first keyframe, as the encoder advertises them.
    unsafe fn sequence_header(&self) -> Result<Vec<u8>> {
        let current = self.transform.GetOutputCurrentType(0)?;
        let mut header = vec![0u8; current.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER)? as usize];
        current.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut header, None)?;
        Ok(header)
    }

    /// Takes the next encoded picture, or `None` when the encoder wants more input first.
    unsafe fn take_output(&self) -> Result<Option<Vec<u8>>> {
        let info = self.transform.GetOutputStreamInfo(0)?;
        let allocates = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32 != 0;
        let ours = if allocates {
            None
        } else {
            let sample = MFCreateSample()?;
            sample.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(self.frame_bytes))?)?;
            Some(sample)
        };

        let mut output = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(ours),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        }];
        let mut status = 0;
        let result = self.transform.ProcessOutput(0, &mut output, &mut status);
        let sample = ManuallyDrop::take(&mut output[0].pSample);
        ManuallyDrop::drop(&mut output[0].pEvents);

        match result {
            Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
            // The encoder settled on a different output format; take the one it offers and carry on.
            Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                let offered = self.transform.GetOutputAvailableType(0, 0)?;
                self.transform.SetOutputType(0, &offered, 0)?;
                return Ok(None);
            }
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        let Some(sample) = sample else { return Ok(None) };

        let buffer = sample.ConvertToContiguousBuffer()?;
        let (mut source, mut length) = (std::ptr::null_mut(), 0);
        buffer.Lock(&mut source, None, Some(&mut length))?;
        let mut data = std::slice::from_raw_parts(source, length as usize).to_vec();
        buffer.Unlock()?;

        let keyframe = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
        if keyframe && !has_parameter_sets(&data) {
            let mut with_header = self.sequence_header()?;
            with_header.append(&mut data);
            data = with_header;
        }
        Ok(Some(data))
    }
}
