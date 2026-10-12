//! Screen capture through Windows Graphics Capture.

use std::mem::ManuallyDrop;

use super::hdr::{display_mode, ToneMapper};

use windows::{
    core::{Error, Interface, Result},
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{E_FAIL, HMODULE},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, ID3D11VideoContext,
                ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
                D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11_VIDEO_PROCESSOR_CAPS,
                D3D11_VIDEO_PROCESSOR_FILTER_CAPS_EDGE_ENHANCEMENT, D3D11_VIDEO_PROCESSOR_FILTER_EDGE_ENHANCEMENT,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_COLOR_SPACE,
                D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
                D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
                D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
                D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D,
            },
            Dxgi::{
                Common::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC},
                IDXGIDevice,
            },
            Gdi::HMONITOR,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
};

/// Output colors: BT.709 with limited (16 to 235) range, which is what browsers assume for HD video.
/// The bits are `YCbCr_Matrix = 1` and `Nominal_Range = 1` of `D3D11_VIDEO_PROCESSOR_COLOR_SPACE`.
const BT709_LIMITED: D3D11_VIDEO_PROCESSOR_COLOR_SPACE = D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0b01_0100 };
/// Input colors: full-range RGB, as the desktop is drawn.
const RGB_FULL: D3D11_VIDEO_PROCESSOR_COLOR_SPACE = D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 };

/// One monitor, read into NV12 pictures no larger than the size asked for.
///
/// The graphics card's video processor does the shrinking and the color conversion, so text stays
/// smooth and the CPU only copies the finished picture.
pub struct Screen {
    context: ID3D11DeviceContext,
    video_context: ID3D11VideoContext,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    processor: ID3D11VideoProcessor,
    /// The captured picture, in a texture the video processor may read.
    input: ID3D11Texture2D,
    input_view: ID3D11VideoProcessorInputView,
    /// The converted picture on the graphics card.
    output: ID3D11Texture2D,
    output_view: ID3D11VideoProcessorOutputView,
    /// A copy of `output` that the CPU is allowed to read.
    staging: ID3D11Texture2D,
    source: SizeInt32,
    target: (u32, u32),
    /// Present when the screen is in HDR mode: what brings its picture down to standard range.
    hdr: Option<Hdr>,
    /// The monitor no longer matches what this capture was set up for (it changed resolution).
    stale: bool,
}

struct Hdr {
    device: ID3D11Device,
    monitor: HMONITOR,
    tone_mapper: ToneMapper,
    /// The linear value SDR white is drawn at, which follows a slider in Windows' settings.
    white: f32,
    pictures_since_check: u32,
}

/// How many pictures pass between looks at the SDR brightness setting: a couple of seconds' worth.
const WHITE_CHECK_EVERY: u32 = 60;

/// The largest even-sided size with the source's shape that fits inside `max`.
fn fit(source: SizeInt32, max: (u32, u32)) -> (u32, u32) {
    let (width, height) = (source.Width as u64, source.Height as u64);
    let (mut fit_width, mut fit_height) = (width, height);
    if fit_width > max.0 as u64 {
        (fit_width, fit_height) = (max.0 as u64, height * max.0 as u64 / width);
    }
    if fit_height > max.1 as u64 {
        (fit_width, fit_height) = (width * max.1 as u64 / height, max.1 as u64);
    }
    ((fit_width as u32 & !1).max(2), (fit_height as u32 & !1).max(2))
}

unsafe fn texture(
    device: &ID3D11Device,
    size: (u32, u32),
    format: DXGI_FORMAT,
    usage: D3D11_USAGE,
    bind: u32,
    cpu_access: u32,
) -> Result<ID3D11Texture2D> {
    let description = D3D11_TEXTURE2D_DESC {
        Width: size.0,
        Height: size.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: usage,
        BindFlags: bind,
        CPUAccessFlags: cpu_access,
        MiscFlags: 0,
    };
    let mut created = None;
    device.CreateTexture2D(&description, None, Some(&mut created))?;
    created.ok_or_else(|| Error::from(E_FAIL))
}

impl Screen {
    /// With `sharpen`, edges are enhanced when the picture is shrunk, which keeps small text legible.
    pub fn open(monitor: HMONITOR, max: (u32, u32), fps: u32, sharpen: bool) -> Result<Self> {
        unsafe {
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device: ID3D11Device = device.ok_or_else(|| Error::from(E_FAIL))?;
            let context: ID3D11DeviceContext = context.ok_or_else(|| Error::from(E_FAIL))?;

            let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
            let item: GraphicsCaptureItem = interop.CreateForMonitor(monitor)?;
            let source = item.Size()?;
            if source.Width < 2 || source.Height < 2 {
                return Err(Error::new(E_FAIL, "the screen has no size"));
            }
            let source_size = (source.Width as u32, source.Height as u32);
            let target = fit(source, max);

            // The video processor: full-size BGRA in, NV12 at the target size out.
            let video_device: ID3D11VideoDevice = device.cast()?;
            let video_context: ID3D11VideoContext = context.cast()?;
            let rate = DXGI_RATIONAL { Numerator: fps, Denominator: 1 };
            let enumerator = video_device.CreateVideoProcessorEnumerator(&D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: source_size.0,
                InputHeight: source_size.1,
                OutputFrameRate: rate,
                OutputWidth: target.0,
                OutputHeight: target.1,
                Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            })?;
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;
            video_context.VideoProcessorSetStreamFrameFormat(&processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            video_context.VideoProcessorSetStreamColorSpace(&processor, 0, &RGB_FULL);
            video_context.VideoProcessorSetOutputColorSpace(&processor, &BT709_LIMITED);
            // No driver "enhancements" of its own choosing: the picture must be the screen.
            video_context.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            // Shrinking softens small text; a moderate edge enhancement, where the graphics card has one,
            // gives back some of its crispness. Not applied at the screen's own size, where it would only
            // add halos.
            if sharpen && target != source_size {
                let mut caps = D3D11_VIDEO_PROCESSOR_CAPS::default();
                let can = enumerator.GetVideoProcessorCaps(&mut caps).is_ok()
                    && caps.FilterCaps & D3D11_VIDEO_PROCESSOR_FILTER_CAPS_EDGE_ENHANCEMENT.0 as u32 != 0;
                if let (true, Ok(range)) = (can, enumerator.GetVideoProcessorFilterRange(D3D11_VIDEO_PROCESSOR_FILTER_EDGE_ENHANCEMENT)) {
                    let level = range.Minimum + (range.Maximum - range.Minimum) / 3;
                    video_context.VideoProcessorSetStreamFilter(&processor, 0, D3D11_VIDEO_PROCESSOR_FILTER_EDGE_ENHANCEMENT, true, level);
                }
            }

            let render_target = D3D11_BIND_RENDER_TARGET.0 as u32;
            let input = texture(&device, source_size, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT, render_target, 0)?;
            let mut input_view = None;
            video_device.CreateVideoProcessorInputView(
                &input,
                &enumerator,
                &D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                    FourCC: 0,
                    ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
                },
                Some(&mut input_view),
            )?;
            let input_view = input_view.ok_or_else(|| Error::from(E_FAIL))?;

            let output = texture(&device, target, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT, render_target, 0)?;
            let mut output_view = None;
            video_device.CreateVideoProcessorOutputView(
                &output,
                &enumerator,
                &D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                    ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
                },
                Some(&mut output_view),
            )?;
            let output_view = output_view.ok_or_else(|| Error::from(E_FAIL))?;

            let staging = texture(&device, target, DXGI_FORMAT_NV12, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32)?;

            // An HDR screen is captured with its full range and brought down to standard range here.
            // Captured as plain 8-bit, everything brighter than white would simply be cut off.
            let mode = display_mode(monitor);
            let hdr = match mode.hdr.then(|| ToneMapper::new(&device, &input, source_size)) {
                Some(Ok(tone_mapper)) => Some(Hdr { device: device.clone(), monitor, tone_mapper, white: mode.white, pictures_since_check: 0 }),
                Some(Err(error)) => {
                    eprintln!("HDR is on but cannot be converted, so bright areas will look too bright: {error}");
                    None
                }
                None => None,
            };
            let format = if hdr.is_some() { DirectXPixelFormat::R16G16B16A16Float } else { DirectXPixelFormat::B8G8R8A8UIntNormalized };
            eprintln!(
                "Sharing a {}×{} screen as {}×{}{}",
                source.Width,
                source.Height,
                target.0,
                target.1,
                if hdr.is_some() { format!(" in HDR, with SDR white at {:.0} nits", mode.white * 80.0) } else { String::new() }
            );

            let winrt_device: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)?.cast()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt_device, format, 2, source)?;
            let session = pool.CreateCaptureSession(&item)?;
            // Windows 11 can drop the yellow border; older versions keep it.
            let _ = session.SetIsBorderRequired(false);
            let _ = session.SetIsCursorCaptureEnabled(true);
            session.StartCapture()?;

            Ok(Self { context, video_context, pool, session, processor, input, input_view, output, output_view, staging, source, target, hdr, stale: false })
        }
    }

    /// Whether the mouse cursor is drawn into the pictures.
    pub fn show_cursor(&self, visible: bool) {
        let _ = self.session.SetIsCursorCaptureEnabled(visible);
    }

    /// Whether the monitor has changed resolution since this capture was set up; it then has to be set up again.
    pub fn stale(&self) -> bool {
        self.stale
    }

    /// Whether this capture converts from HDR.
    pub fn is_hdr(&self) -> bool {
        self.hdr.is_some()
    }

    /// The size of the pictures `read_latest` produces.
    pub fn size(&self) -> (u32, u32) {
        self.target
    }

    /// Overwrites `nv12` with the newest picture of the screen. Returns `false`, leaving it untouched,
    /// when the screen has not changed since the last call.
    pub fn read_latest(&mut self, nv12: &mut [u8]) -> Result<bool> {
        let mut latest = None;
        while let Ok(frame) = self.pool.TryGetNextFrame() {
            if let Some(older) = latest.replace(frame) {
                let _ = older.Close();
            }
        }
        let Some(frame) = latest else { return Ok(false) };

        if frame.ContentSize()? != self.source {
            self.stale = true;
            let _ = frame.Close();
            return Ok(false);
        }

        unsafe {
            let captured: ID3D11Texture2D = frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()?;
            match &mut self.hdr {
                Some(hdr) => {
                    // The sharer may move the SDR brightness slider during a session.
                    hdr.pictures_since_check += 1;
                    if hdr.pictures_since_check >= WHITE_CHECK_EVERY {
                        hdr.pictures_since_check = 0;
                        hdr.white = display_mode(hdr.monitor).white;
                    }
                    hdr.tone_mapper.draw(&hdr.device, &self.context, &captured, hdr.white)?;
                }
                None => self.context.CopyResource(&self.input, &captured),
            }

            let mut streams = [D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(self.input_view.clone())),
                ..Default::default()
            }];
            let converted = self.video_context.VideoProcessorBlt(&self.processor, &self.output_view, 0, &streams);
            ManuallyDrop::drop(&mut streams[0].pInputSurface);
            converted?;

            self.context.CopyResource(&self.staging, &self.output);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            self.copy_out(mapped.pData as *const u8, mapped.RowPitch as usize, nv12);
            self.context.Unmap(&self.staging, 0);
        }
        let _ = frame.Close();
        Ok(true)
    }

    /// Copies a mapped NV12 texture, whose rows are padded to `pitch` bytes, into a tightly packed buffer:
    /// the Y plane, then the half-height interleaved UV plane.
    unsafe fn copy_out(&self, mapped: *const u8, pitch: usize, nv12: &mut [u8]) {
        let (width, height) = (self.target.0 as usize, self.target.1 as usize);
        for (row, packed) in nv12.chunks_exact_mut(width).take(height * 3 / 2).enumerate() {
            packed.copy_from_slice(std::slice::from_raw_parts(mapped.add(row * pitch), width));
        }
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
