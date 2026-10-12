//! The parts of macOS's C frameworks that more than one piece of the agent calls: Core Foundation's
//! containers, the permissions a Mac asks its user for, and the list of windows on screen.

#![allow(non_upper_case_globals, non_snake_case)]

use std::ffi::c_void;

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFArrayRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

impl CGRect {
    pub fn contains(&self, point: CGPoint) -> bool {
        point.x >= self.origin.x && point.x < self.origin.x + self.size.width && point.y >= self.origin.y && point.y < self.origin.y + self.size.height
    }
}

const UTF8: u32 = 0x0800_0100;
/// `kCFNumberSInt64Type`
const SINT64: isize = 4;
/// `kCFNumberFloat64Type`
const FLOAT64: isize = 6;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFBooleanTrue: CFTypeRef;
    pub static kCFBooleanFalse: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;

    pub fn CFRelease(object: CFTypeRef);
    pub fn CFRetain(object: CFTypeRef) -> CFTypeRef;
    fn CFStringCreateWithBytes(allocator: CFTypeRef, bytes: *const u8, length: isize, encoding: u32, external: u8) -> CFStringRef;
    fn CFStringGetLength(string: CFStringRef) -> isize;
    fn CFStringGetCString(string: CFStringRef, buffer: *mut u8, size: isize, encoding: u32) -> u8;
    fn CFNumberCreate(allocator: CFTypeRef, kind: isize, value: *const c_void) -> CFTypeRef;
    fn CFNumberGetValue(number: CFTypeRef, kind: isize, value: *mut c_void) -> u8;
    fn CFBooleanGetValue(boolean: CFTypeRef) -> u8;
    fn CFDictionaryCreateMutable(allocator: CFTypeRef, capacity: isize, keys: *const c_void, values: *const c_void) -> CFDictionaryRef;
    fn CFDictionarySetValue(dictionary: CFDictionaryRef, key: CFTypeRef, value: CFTypeRef);
    pub fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    pub fn CFArrayGetCount(array: CFArrayRef) -> isize;
    pub fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> CFTypeRef;
}

/// Something of Core Foundation's that this code owns, and lets go of when it is done with it.
pub struct Cf(pub CFTypeRef);

// Core Foundation's objects may be used from any thread, one at a time.
unsafe impl Send for Cf {}

impl Cf {
    /// `None` for the null that Core Foundation returns when it could not make something.
    pub fn owned(object: CFTypeRef) -> Option<Self> {
        (!object.is_null()).then_some(Self(object))
    }

    pub fn string(text: &str) -> Self {
        Self(unsafe { CFStringCreateWithBytes(std::ptr::null(), text.as_ptr(), text.len() as isize, UTF8, 0) })
    }

    pub fn integer(value: i64) -> Self {
        Self(unsafe { CFNumberCreate(std::ptr::null(), SINT64, (&value as *const i64).cast()) })
    }

    /// A dictionary, which keeps its own hold on what is put in it.
    pub fn dictionary(entries: &[(CFTypeRef, CFTypeRef)]) -> Self {
        unsafe {
            let dictionary = CFDictionaryCreateMutable(std::ptr::null(), 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            for (key, value) in entries {
                CFDictionarySetValue(dictionary, *key, *value);
            }
            Self(dictionary)
        }
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

pub fn boolean(value: bool) -> CFTypeRef {
    unsafe {
        if value {
            kCFBooleanTrue
        } else {
            kCFBooleanFalse
        }
    }
}

/// The whole number a `CFNumber` holds.
pub unsafe fn integer(number: CFTypeRef) -> Option<i64> {
    let mut value = 0i64;
    (!number.is_null() && CFNumberGetValue(number, SINT64, (&mut value as *mut i64).cast()) != 0).then_some(value)
}

pub unsafe fn float(number: CFTypeRef) -> Option<f64> {
    let mut value = 0f64;
    (!number.is_null() && CFNumberGetValue(number, FLOAT64, (&mut value as *mut f64).cast()) != 0).then_some(value)
}

pub unsafe fn is_true(boolean: CFTypeRef) -> bool {
    !boolean.is_null() && CFBooleanGetValue(boolean) != 0
}

pub unsafe fn text(string: CFStringRef) -> Option<String> {
    if string.is_null() {
        return None;
    }
    // Room for every character at its longest in UTF-8, and the zero that ends it.
    let mut buffer = vec![0u8; CFStringGetLength(string) as usize * 4 + 1];
    if CFStringGetCString(string, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) == 0 {
        return None;
    }
    let length = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
    buffer.truncate(length);
    String::from_utf8(buffer).ok()
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGWindowBounds: CFStringRef;
    static kCGWindowOwnerPID: CFStringRef;
    static kCGWindowLayer: CFStringRef;
    static kCGWindowAlpha: CFStringRef;
    static kCGWindowNumber: CFStringRef;

    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFArrayRef;
    fn CGRectMakeWithDictionaryRepresentation(dictionary: CFDictionaryRef, rect: *mut CGRect) -> bool;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;

    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

/// A window on screen, as the window server describes it.
pub struct OnScreen {
    pub id: u32,
    pub owner: i32,
    /// 0 for ordinary windows; above it, those that float over them.
    pub layer: i64,
    /// In points, with the origin at the top left of the main display.
    pub bounds: CGRect,
}

/// Every window on screen that can be seen, the one in front first.
pub fn windows_on_screen() -> Vec<OnScreen> {
    /// `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements`
    const ON_SCREEN: u32 = 1 | 16;
    let mut found = Vec::new();
    unsafe {
        let Some(list) = Cf::owned(CGWindowListCopyWindowInfo(ON_SCREEN, 0)) else { return found };
        for index in 0..CFArrayGetCount(list.0) {
            let window = CFArrayGetValueAtIndex(list.0, index);
            let number = |key| integer(CFDictionaryGetValue(window, key));
            let mut bounds = CGRect::default();
            let placed = CGRectMakeWithDictionaryRepresentation(CFDictionaryGetValue(window, kCGWindowBounds), &mut bounds);
            let visible = float(CFDictionaryGetValue(window, kCGWindowAlpha)).is_none_or(|alpha| alpha > 0.0);
            if let (true, true, Some(id), Some(owner)) = (placed, visible, number(kCGWindowNumber), number(kCGWindowOwnerPID)) {
                found.push(OnScreen { id: id as u32, owner: owner as i32, layer: number(kCGWindowLayer).unwrap_or(0), bounds });
            }
        }
    }
    found
}

/// Whether macOS lets this app see the screen. The answer is fixed when the app starts: after the
/// user allows it, the app has to be opened again.
pub fn may_record_screen() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Asks the user to let this app see the screen. macOS asks once; after that it only adds the app to
/// the list in System Settings, where the user turns it on.
pub fn ask_to_record_screen() {
    use block2::RcBlock;
    use objc2_foundation::NSError;
    use objc2_screen_capture_kit::SCShareableContent;

    unsafe {
        CGRequestScreenCaptureAccess();
        // Asking alone does not always put the app in System Settings' list. Trying to look at what
        // is on screen does: it is what macOS is guarding, and the attempt is what it records.
        let ignored = RcBlock::new(|_content: *mut SCShareableContent, _error: *mut NSError| {});
        SCShareableContent::getShareableContentWithCompletionHandler(&ignored);
    }
}

/// Whether macOS lets this app use the mouse and keyboard, and notice the user's own.
pub fn may_control() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Asks the user to let this app use the mouse and keyboard, which shows them where to allow it.
pub fn ask_to_control() {
    unsafe {
        let options = Cf::dictionary(&[(kAXTrustedCheckOptionPrompt, kCFBooleanTrue)]);
        AXIsProcessTrustedWithOptions(options.0);
    }
}

/// Opens System Settings at one of its Privacy & Security lists.
pub fn open_privacy_settings(list: &str) {
    let _ = std::process::Command::new("/usr/bin/open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{list}")).spawn();
}

/// Whether the lock screen or a screen saver has the display.
pub fn screen_locked() -> bool {
    unsafe {
        let Some(session) = Cf::owned(CGSessionCopyCurrentDictionary()) else { return false };
        let locked = Cf::string("CGSSessionScreenIsLocked");
        is_true(CFDictionaryGetValue(session.0, locked.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints the windows of a running Light Assist as this code sees them: `cargo test -- --ignored windows --nocapture`.
    #[test]
    #[ignore]
    fn windows_of_the_running_app() {
        let app: i32 = std::env::var("APP_PID").expect("APP_PID").parse().unwrap();
        let all = windows_on_screen();
        eprintln!("{} windows on screen", all.len());
        for (index, window) in all.iter().enumerate().filter(|(_, window)| window.owner == app) {
            eprintln!("#{index} id {} layer {} at {:?}", window.id, window.layer, window.bounds);
        }
    }
}
