//! The sharer's monitors, so that the helper can choose which one to see.

use windows::Win32::{
    Foundation::{LPARAM, RECT},
    Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW},
};
use windows::core::BOOL;

/// `MONITORINFOF_PRIMARY`: the monitor with the taskbar's start button and the desktop's origin.
const PRIMARY: u32 = 1;

#[derive(Clone, PartialEq, Eq)]
pub struct Monitor {
    /// Windows' name for it, such as `\\.\DISPLAY1`; stable for as long as the monitor stays connected.
    pub id: String,
    pub handle: HMONITOR,
    /// Where it sits on the desktop, in physical pixels.
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

unsafe extern "system" fn collect(handle: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> BOOL {
    let list = &mut *(list.0 as *mut Vec<Monitor>);
    let mut info = MONITORINFOEXW { monitorInfo: MONITORINFO { cbSize: size_of::<MONITORINFOEXW>() as u32, ..Default::default() }, ..Default::default() };
    if GetMonitorInfoW(handle, &mut info.monitorInfo).as_bool() {
        let name = &info.szDevice;
        let length = name.iter().position(|unit| *unit == 0).unwrap_or(name.len());
        let area = info.monitorInfo.rcMonitor;
        list.push(Monitor {
            id: String::from_utf16_lossy(&name[..length]),
            handle,
            left: area.left,
            top: area.top,
            width: area.right - area.left,
            height: area.bottom - area.top,
            primary: info.monitorInfo.dwFlags & PRIMARY != 0,
        });
    }
    true.into()
}

/// Every connected monitor: the primary one first, then the rest from left to right.
pub fn list() -> Vec<Monitor> {
    let mut list: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by_key(|monitor| (!monitor.primary, monitor.left, monitor.top));
    list
}
