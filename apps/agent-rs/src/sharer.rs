//! Notices when the sharer uses their own mouse or keyboard, so that the helper's input can give way.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use crate::util::now_ms;

/// When the sharer last touched their mouse or keyboard, in milliseconds since the epoch.
static LAST_INPUT: AtomicU64 = AtomicU64::new(0);

/// Watches the sharer's own input for as long as it lives. Only one is ever alive: one helper, one session.
pub struct SharerWatch {
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    thread: u32,
}

impl SharerWatch {
    pub fn start() -> Self {
        LAST_INPUT.store(0, Ordering::Relaxed);
        Self { thread: os::start() }
    }

    /// Whether the sharer used their mouse or keyboard within the last `window`.
    pub fn sharer_active(&self, window: Duration) -> bool {
        now_ms().saturating_sub(LAST_INPUT.load(Ordering::Relaxed)) < window.as_millis() as u64
    }
}

impl Drop for SharerWatch {
    fn drop(&mut self) {
        os::stop(self.thread);
    }
}

#[cfg(windows)]
mod os {
    use std::sync::{atomic::Ordering, mpsc};

    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
            LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_QUIT,
        },
    };

    use super::LAST_INPUT;
    use crate::util::now_ms;

    // Windows marks input that a program injected, which is what tells the sharer's own hands apart
    // from the helper's input (and from anything else that types for them).

    unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && (*(lparam.0 as *const MSLLHOOKSTRUCT)).flags & LLMHF_INJECTED == 0 {
            LAST_INPUT.store(now_ms(), Ordering::Relaxed);
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && !(*(lparam.0 as *const KBDLLHOOKSTRUCT)).flags.contains(LLKHF_INJECTED) {
            LAST_INPUT.store(now_ms(), Ordering::Relaxed);
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Low-level hooks are delivered to the thread that installed them, which therefore has to pump
    /// messages. Returns that thread's id, or 0 if it could not be started.
    pub fn start() -> u32 {
        let (ready, started) = mpsc::channel();
        std::thread::spawn(move || unsafe {
            let hooks = [
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0),
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0),
            ];
            let _ = ready.send(GetCurrentThreadId());

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {}

            for hook in hooks.into_iter().flatten() {
                let _ = UnhookWindowsHookEx(hook);
            }
        });
        started.recv().unwrap_or(0)
    }

    pub fn stop(thread: u32) {
        if thread != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod os {
    use std::{
        ffi::c_void,
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
    };

    use super::LAST_INPUT;
    use crate::{
        mac::{may_control, CFRelease, CFStringRef},
        util::now_ms,
    };

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events: u64,
            callback: extern "C" fn(*const c_void, u32, *const c_void, *mut c_void) -> *const c_void,
            context: *mut c_void,
        ) -> *const c_void;
        fn CGEventTapEnable(tap: *const c_void, enable: bool);
        fn CGEventGetIntegerValueField(event: *const c_void, field: u32) -> i64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopCommonModes: CFStringRef;
        fn CFMachPortCreateRunLoopSource(allocator: *const c_void, port: *const c_void, order: isize) -> *const c_void;
        fn CFRunLoopGetCurrent() -> *const c_void;
        fn CFRunLoopAddSource(run_loop: *const c_void, source: *const c_void, mode: CFStringRef);
        fn CFRunLoopRun();
        fn CFRunLoopStop(run_loop: *const c_void);
    }

    /// Everything the user's session receives, after the hardware: `kCGSessionEventTap`.
    const SESSION: u32 = 1;
    /// Only watching: `kCGEventTapOptionListenOnly`.
    const LISTEN_ONLY: u32 = 1;
    /// Mouse buttons, movement and dragging, keys, modifier keys and scrolling, by `CGEventType` number.
    const WATCHED: u64 = (0b1111_1110) | (0b111 << 10) | (1 << 22) | (0b111 << 25);
    /// macOS switches a tap off when it is slow to answer, and says so with one of these.
    const SWITCHED_OFF: [u32; 2] = [0xffff_fffe, 0xffff_ffff];
    /// `kCGEventSourceUserData`, where the helper's input carries its mark.
    const USER_DATA: u32 = 42;
    const HELPERS_MARK: i64 = 0x4c41_4853;

    /// The watching thread's run loop and its tap, as numbers, which may be shared between threads.
    static RUN_LOOP: AtomicUsize = AtomicUsize::new(0);
    static TAP: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn seen(_proxy: *const c_void, kind: u32, event: *const c_void, _context: *mut c_void) -> *const c_void {
        if SWITCHED_OFF.contains(&kind) {
            let tap = TAP.load(Ordering::Relaxed) as *const c_void;
            if !tap.is_null() {
                unsafe { CGEventTapEnable(tap, true) };
            }
        } else if unsafe { CGEventGetIntegerValueField(event, USER_DATA) } != HELPERS_MARK {
            LAST_INPUT.store(now_ms(), Ordering::Relaxed);
        }
        event
    }

    /// A tap is served by the run loop of the thread that added it. Returns 1 once it is watching, or
    /// 0 if macOS does not let the app watch, in which case it does not let the helper act either.
    pub fn start() -> u32 {
        if !may_control() {
            return 0;
        }
        let (ready, started) = mpsc::channel();
        std::thread::spawn(move || unsafe {
            let tap = CGEventTapCreate(SESSION, 0, LISTEN_ONLY, WATCHED, seen, std::ptr::null_mut());
            if tap.is_null() {
                let _ = ready.send(0);
                return;
            }
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            TAP.store(tap as usize, Ordering::Relaxed);
            RUN_LOOP.store(run_loop as usize, Ordering::Relaxed);
            let _ = ready.send(1);

            CFRunLoopRun();

            TAP.store(0, Ordering::Relaxed);
            CGEventTapEnable(tap, false);
            CFRelease(source);
            CFRelease(tap);
        });
        started.recv().unwrap_or(0)
    }

    pub fn stop(thread: u32) {
        let run_loop = RUN_LOOP.swap(0, Ordering::Relaxed) as *const c_void;
        if thread != 0 && !run_loop.is_null() {
            unsafe { CFRunLoopStop(run_loop) };
        }
    }
}

/// Elsewhere the sharer's input cannot be told apart so far, and the helper is never made to wait.
#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    pub fn start() -> u32 {
        0
    }

    pub fn stop(_thread: u32) {}
}
