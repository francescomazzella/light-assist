//! The sharer's displays, so that the helper can choose which one to see.

use std::ffi::c_void;

use crate::mac::CGRect;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGGetActiveDisplayList(room: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGDisplayCopyDisplayMode(display: u32) -> *const c_void;
    fn CGDisplayModeGetPixelWidth(mode: *const c_void) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: *const c_void) -> usize;
    fn CGDisplayModeRelease(mode: *const c_void);
}

#[derive(Clone, PartialEq, Eq)]
pub struct Monitor {
    /// macOS's number for it, as text; stable for as long as the display stays connected.
    pub id: String,
    pub display: u32,
    /// Where it sits on the desktop, in points, which is what the mouse is placed in.
    pub left: i32,
    pub top: i32,
    pub points_wide: i32,
    pub points_high: i32,
    /// Its size in pixels, which on a Retina display is twice that in points.
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

/// Every connected display: the main one first, then the rest from left to right.
pub fn list() -> Vec<Monitor> {
    let mut displays = [0u32; 16];
    let mut count = 0;
    if unsafe { CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    let main = unsafe { CGMainDisplayID() };
    let mut list: Vec<Monitor> = displays[..count as usize]
        .iter()
        .map(|&display| unsafe {
            let area = CGDisplayBounds(display);
            let mode = CGDisplayCopyDisplayMode(display);
            // A display without a mode is one being unplugged; its size in points will do until it is gone.
            let (width, height) = if mode.is_null() {
                (area.size.width as usize, area.size.height as usize)
            } else {
                let size = (CGDisplayModeGetPixelWidth(mode), CGDisplayModeGetPixelHeight(mode));
                CGDisplayModeRelease(mode);
                size
            };
            Monitor {
                id: display.to_string(),
                display,
                left: area.origin.x as i32,
                top: area.origin.y as i32,
                points_wide: area.size.width as i32,
                points_high: area.size.height as i32,
                width: width as i32,
                height: height as i32,
                primary: display == main,
            }
        })
        .collect();
    list.sort_by_key(|monitor| (!monitor.primary, monitor.left, monitor.top));
    list
}
