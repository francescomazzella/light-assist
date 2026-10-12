//! A keyboard shortcut that ends the session from anywhere: the sharer's quickest way out.

/// Shown to the sharer wherever the shortcut is mentioned. The same three keys on every keyboard;
/// a Mac has other names for them.
pub const STOP_KEY_NAME: &str = if cfg!(target_os = "macos") { "Control+Option+Delete" } else { "Ctrl+Alt+Backspace" };

/// Holds the shortcut for as long as it lives. It works whichever window has the keyboard.
pub struct StopKey {
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    thread: u32,
}

impl StopKey {
    /// Registers the shortcut; `on_press` runs each time it is pressed. The shortcut may be unavailable
    /// (another program holds it), in which case the Stop buttons are still there.
    pub fn register(on_press: impl Fn() + Send + 'static) -> Self {
        Self { thread: os::start(Box::new(on_press)) }
    }
}

impl Drop for StopKey {
    fn drop(&mut self) {
        os::stop(self.thread);
    }
}

#[cfg(windows)]
mod os {
    use std::sync::mpsc;

    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_BACK},
            WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT},
        },
    };

    const HOTKEY_ID: i32 = 1;

    /// A hotkey without a window is delivered to the thread that registered it, which therefore has to
    /// pump messages. Returns that thread's id, or 0 if it could not be started.
    pub fn start(on_press: Box<dyn Fn() + Send>) -> u32 {
        let (ready, started) = mpsc::channel();
        std::thread::spawn(move || unsafe {
            let registered = RegisterHotKey(None, HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_BACK.0 as u32);
            if let Err(error) = &registered {
                eprintln!("The stop shortcut is unavailable: {error}");
            }
            let _ = ready.send(GetCurrentThreadId());

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                if message.message == WM_HOTKEY {
                    on_press();
                }
            }
            if registered.is_ok() {
                let _ = UnregisterHotKey(None, HOTKEY_ID);
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
        sync::{Arc, Mutex},
    };

    use crate::util::lock;

    #[repr(C)]
    struct EventTypeSpec {
        class: u32,
        kind: u32,
    }

    #[repr(C)]
    struct EventHotKeyID {
        signature: u32,
        id: u32,
    }

    type Handler = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetEventDispatcherTarget() -> *mut c_void;
        fn InstallEventHandler(target: *mut c_void, handler: Handler, count: usize, kinds: *const EventTypeSpec, context: *mut c_void, installed: *mut *mut c_void) -> i32;
        fn RegisterEventHotKey(key: u32, modifiers: u32, id: EventHotKeyID, target: *mut c_void, options: u32, registered: *mut *mut c_void) -> i32;
        fn UnregisterEventHotKey(registered: *mut c_void) -> i32;
    }

    extern "C" {
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
    }

    /// `kEventClassKeyboard` and `kEventHotKeyPressed`.
    const HOT_KEY_PRESSED: EventTypeSpec = EventTypeSpec { class: u32::from_be_bytes(*b"keyb"), kind: 5 };
    /// The key that deletes backwards, where a PC keyboard has Backspace.
    const DELETE: u32 = 0x33;
    /// `controlKey | optionKey`
    const CONTROL_OPTION: u32 = 0x1000 | 0x0800;

    type Callback = Arc<Mutex<Box<dyn Fn() + Send>>>;

    /// What to run when the shortcut is pressed, while it is held.
    static ON_PRESS: Mutex<Option<Callback>> = Mutex::new(None);
    /// The registered shortcut, as a number: only the main thread makes a pointer of it again.
    static REGISTERED: Mutex<usize> = Mutex::new(0);
    static HANDLER_INSTALLED: Mutex<bool> = Mutex::new(false);

    extern "C" fn pressed(_next: *mut c_void, _event: *mut c_void, _context: *mut c_void) -> i32 {
        // Ending the session lets go of the shortcut, which wants the lock taken here: so it is given
        // back before the callback runs.
        let callback = lock(&ON_PRESS).clone();
        if let Some(callback) = callback {
            (lock(&callback))();
        }
        0
    }

    extern "C" fn register(_context: *mut c_void) {
        unsafe {
            let mut installed = lock(&HANDLER_INSTALLED);
            if !*installed {
                // Installed once and left in place: it does nothing while no shortcut is registered.
                *installed = InstallEventHandler(GetEventDispatcherTarget(), pressed, 1, &HOT_KEY_PRESSED, std::ptr::null_mut(), std::ptr::null_mut()) == 0;
            }
            let mut registered = lock(&REGISTERED);
            // Let go of again before this ran, or still held from the last session.
            if lock(&ON_PRESS).is_none() || *registered != 0 {
                return;
            }
            let mut hot_key = std::ptr::null_mut();
            let id = EventHotKeyID { signature: u32::from_be_bytes(*b"LAst"), id: 1 };
            let status = RegisterEventHotKey(DELETE, CONTROL_OPTION, id, GetEventDispatcherTarget(), 0, &mut hot_key);
            if status != 0 {
                eprintln!("The stop shortcut is unavailable ({status})");
                return;
            }
            *registered = hot_key as usize;
        }
    }

    extern "C" fn unregister(_context: *mut c_void) {
        let mut registered = lock(&REGISTERED);
        if *registered != 0 && lock(&ON_PRESS).is_none() {
            unsafe { UnregisterEventHotKey(*registered as *mut c_void) };
            *registered = 0;
        }
    }

    /// macOS delivers the shortcut through the app's own event loop, on the main thread, where it
    /// also has to be registered. An agent without a window has no such loop, and no shortcut.
    pub fn start(on_press: Box<dyn Fn() + Send>) -> u32 {
        *lock(&ON_PRESS) = Some(Arc::new(Mutex::new(on_press)));
        unsafe { dispatch_async_f(&_dispatch_main_q, std::ptr::null_mut(), register) };
        1
    }

    pub fn stop(_thread: u32) {
        *lock(&ON_PRESS) = None;
        unsafe { dispatch_async_f(&_dispatch_main_q, std::ptr::null_mut(), unregister) };
    }
}

/// Elsewhere there is no shortcut so far.
#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    pub fn start(_on_press: Box<dyn Fn() + Send>) -> u32 {
        0
    }

    pub fn stop(_thread: u32) {}
}
