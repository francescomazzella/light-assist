//! High dynamic range: knowing when the screen is in HDR mode, and bringing its picture down to the
//! standard range that the video carries.
//!
//! In HDR mode Windows composes the desktop in linear light, where 1.0 is 80 nits. Ordinary (SDR)
//! content is drawn at the "SDR content brightness" the user chose, often around 200 to 300 nits, so
//! white is somewhere near 3.0; HDR video and games go far above that. Captured as plain 8-bit, all
//! of that above-white light is cut off flat, which is what made HDR video look too bright.

use windows::{
    core::{s, Error, Interface, Result, PCSTR},
    Win32::{
        Devices::Display::{
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
        },
        Foundation::E_FAIL,
        Graphics::{
            Direct3D::{Fxc::D3DCompile, ID3DBlob, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST},
            Direct3D11::{
                ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView,
                ID3D11SamplerState, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BUFFER_DESC,
                D3D11_COMPARISON_NEVER, D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_SAMPLER_DESC, D3D11_TEXTURE_ADDRESS_CLAMP,
                D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
            },
            Dxgi::{
                Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6,
            },
            Gdi::HMONITOR,
        },
    },
};

/// The scale Windows itself uses: a linear value of 1.0 is 80 nits.
const NITS_PER_UNIT: f32 = 80.0;
/// What Windows draws SDR content at when the setting cannot be read.
const DEFAULT_WHITE_NITS: f32 = 240.0;

/// How a monitor is being driven right now.
pub struct DisplayMode {
    pub hdr: bool,
    /// The linear value at which SDR white is drawn: the "SDR content brightness", in units of 80 nits.
    pub white: f32,
}

/// The monitor's name as Windows' older display interfaces know it (`\\.\DISPLAY1`), and whether it is
/// in HDR mode. Nothing is assumed: a monitor that cannot be found is treated as not in HDR.
unsafe fn find(monitor: HMONITOR) -> Option<([u16; 32], bool)> {
    let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
    for adapter in (0..).map_while(|index| factory.EnumAdapters1(index).ok()) {
        for output in (0..).map_while(|index| adapter.EnumOutputs(index).ok()) {
            let Ok(description) = output.cast::<IDXGIOutput6>().and_then(|output| output.GetDesc1()) else { continue };
            if description.Monitor == monitor {
                return Some((description.DeviceName, description.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020));
            }
        }
    }
    None
}

/// The brightness, in nits, that Windows draws SDR white at on the monitor with this name.
unsafe fn sdr_white_nits(name: &[u16; 32]) -> Option<f32> {
    let (mut path_count, mut mode_count) = (0, 0);
    GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count).ok().ok()?;
    let mut paths = vec![Default::default(); path_count as usize];
    let mut modes = vec![Default::default(); mode_count as usize];
    QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut path_count, paths.as_mut_ptr(), &mut mode_count, modes.as_mut_ptr(), None).ok().ok()?;

    for path in paths.iter().take(path_count as usize) {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };
        if DisplayConfigGetDeviceInfo(&mut source.header) != 0 || source.viewGdiDeviceName != *name {
            continue;
        }

        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                size: size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            SDRWhiteLevel: 0,
        };
        // Reported in thousandths of 80 nits.
        return (DisplayConfigGetDeviceInfo(&mut white.header) == 0 && white.SDRWhiteLevel > 0)
            .then(|| white.SDRWhiteLevel as f32 / 1000.0 * NITS_PER_UNIT);
    }
    None
}

pub fn display_mode(monitor: HMONITOR) -> DisplayMode {
    unsafe {
        let Some((name, hdr)) = find(monitor) else { return DisplayMode { hdr: false, white: 1.0 } };
        let nits = if hdr { sdr_white_nits(&name).unwrap_or(DEFAULT_WHITE_NITS) } else { NITS_PER_UNIT };
        DisplayMode { hdr, white: nits / NITS_PER_UNIT }
    }
}

/// One triangle that covers the whole target, and the conversion applied to every pixel under it.
const SHADER: &str = r"
Texture2D<float4> screen : register(t0);
SamplerState nearest : register(s0);
cbuffer Settings : register(b0) { float white; float3 unused; };

struct Corner { float4 position : SV_Position; float2 at : TEXCOORD0; };

Corner vertex(uint id : SV_VertexID) {
    Corner corner;
    corner.at = float2((id << 1) & 2, id & 2);
    corner.position = float4(corner.at * float2(2, -2) + float2(-1, 1), 0, 1);
    return corner;
}

float3 encode(float3 linear_light) {
    // The sRGB curve, which is what an 8-bit picture is expected to be in.
    return lerp(12.92 * linear_light, 1.055 * pow(linear_light, 1 / 2.4) - 0.055, step(0.0031308, linear_light));
}

float4 pixel(Corner corner) : SV_Target {
    // Colors outside what SDR can show come out negative; they are brought to its edge.
    // After the division, 1.0 is SDR white, exactly as the sharer sees it.
    float3 color = max(screen.Sample(nearest, corner.at).rgb, 0) / white;

    // Everything up to nine tenths of white is left exactly as it is, so the desktop looks the same.
    // Brighter light, which only HDR content has much of, is eased into the last tenth, where
    // cutting it off would turn whole areas flat white.
    const float knee = 0.9;
    float peak = max(color.r, max(color.g, color.b));
    if (peak > knee) {
        float eased = knee + (1 - knee) * (1 - exp(-(peak - knee) / (1 - knee)));
        color *= eased / peak;
    }
    return float4(encode(saturate(color)), 1);
}
";

unsafe fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>> {
    let (mut code, mut errors): (Option<ID3DBlob>, Option<ID3DBlob>) = (None, None);
    let compiled = D3DCompile(SHADER.as_ptr().cast(), SHADER.len(), PCSTR::null(), None, None, entry, target, 0, 0, &mut code, Some(&mut errors));
    let bytes = |blob: &ID3DBlob| std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize()).to_vec();

    match (compiled, code) {
        (Ok(()), Some(code)) => Ok(bytes(&code)),
        (compiled, _) => {
            let reason = errors.map(|errors| String::from_utf8_lossy(&bytes(&errors)).into_owned()).unwrap_or_default();
            Err(Error::new(compiled.err().map_or(E_FAIL, |error| error.code()), format!("the HDR shader did not compile: {reason}")))
        }
    }
}

/// Draws an HDR picture of the screen into an ordinary 8-bit one.
pub struct ToneMapper {
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    settings: ID3D11Buffer,
    target: ID3D11RenderTargetView,
    size: (u32, u32),
}

impl ToneMapper {
    /// `target` is the 8-bit texture to draw into; it and every source picture are `size` pixels.
    pub fn new(device: &ID3D11Device, target: &ID3D11Texture2D, size: (u32, u32)) -> Result<Self> {
        unsafe {
            let (mut vertex, mut pixel, mut sampler, mut settings, mut view) = (None, None, None, None, None);
            device.CreateVertexShader(&compile(s!("vertex"), s!("vs_4_0"))?, None, Some(&mut vertex))?;
            device.CreatePixelShader(&compile(s!("pixel"), s!("ps_4_0"))?, None, Some(&mut pixel))?;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    // Source and target are the same size, so each pixel is read exactly once.
                    Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC { ByteWidth: 16, Usage: D3D11_USAGE_DEFAULT, BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32, ..Default::default() },
                None,
                Some(&mut settings),
            )?;
            device.CreateRenderTargetView(target, None, Some(&mut view))?;

            let missing = || Error::from(E_FAIL);
            Ok(Self {
                vertex: vertex.ok_or_else(missing)?,
                pixel: pixel.ok_or_else(missing)?,
                sampler: sampler.ok_or_else(missing)?,
                settings: settings.ok_or_else(missing)?,
                target: view.ok_or_else(missing)?,
                size,
            })
        }
    }

    /// Converts `source`, a linear-light picture of the screen, with SDR white at the linear value `white`.
    pub fn draw(&self, device: &ID3D11Device, context: &ID3D11DeviceContext, source: &ID3D11Texture2D, white: f32) -> Result<()> {
        unsafe {
            let mut picture = None;
            device.CreateShaderResourceView(source, None, Some(&mut picture))?;

            let settings = [white.max(0.01), 0.0, 0.0, 0.0f32];
            context.UpdateSubresource(&self.settings, 0, None, settings.as_ptr().cast(), 0, 0);

            context.OMSetRenderTargets(Some(&[Some(self.target.clone())]), None);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: self.size.0 as f32,
                Height: self.size.1 as f32,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.vertex, None);
            context.PSSetShader(&self.pixel, None);
            context.PSSetShaderResources(0, Some(&[picture]));
            context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            context.PSSetConstantBuffers(0, Some(&[Some(self.settings.clone())]));
            context.Draw(3, 0);

            // Let go of both pictures, so that the next step may read the one just drawn.
            context.PSSetShaderResources(0, Some(&[None]));
            context.OMSetRenderTargets(None, None);
            Ok(())
        }
    }
}
