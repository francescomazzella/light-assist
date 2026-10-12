//! Turns the helper's mouse and keyboard messages into real input on the sharer's computer.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    protocol::{InputMessage, MouseButton},
    sharer::SharerWatch,
};

/// More than anyone types between two animation frames; longer text is cut short.
const MAX_TEXT_CHARS: usize = 16;
/// How long the helper's input stays paused after the sharer last used their own mouse or keyboard.
const SHARER_PRIORITY: Duration = Duration::from_millis(1000);

/// The part of the desktop the helper is looking at: left, top, width, height, in the units the mouse
/// is placed in (physical pixels on Windows, points on a Mac).
/// Set by the screen capture whenever it starts on a monitor; mouse positions are fractions of it.
static SHARED_AREA: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

pub fn set_shared_area(area: Option<(i32, i32, i32, i32)>) {
    *SHARED_AREA.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = area;
}

fn shared_area() -> Option<(i32, i32, i32, i32)> {
    *SHARED_AREA.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A physical key: its scan code, and whether it is one of the "extended" keys (arrows, right-hand
/// modifiers, the navigation block) that share a scan code with a numpad or left-hand key.
#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    scan: u16,
    extended: bool,
}

/// A physical key, by the number macOS gives its position on the keyboard.
#[cfg(not(windows))]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key(u16);

/// The scan code for a key named as browsers name it. Scan codes describe a position on the keyboard,
/// so the sharer's own layout decides what the key means, exactly as if it were pressed there.
#[cfg(windows)]
fn key(code: &str) -> Option<Key> {
    let plain = |scan| Some(Key { scan, extended: false });
    let extended = |scan| Some(Key { scan, extended: true });

    match code {
        "Escape" => plain(0x01),
        "Digit1" => plain(0x02),
        "Digit2" => plain(0x03),
        "Digit3" => plain(0x04),
        "Digit4" => plain(0x05),
        "Digit5" => plain(0x06),
        "Digit6" => plain(0x07),
        "Digit7" => plain(0x08),
        "Digit8" => plain(0x09),
        "Digit9" => plain(0x0a),
        "Digit0" => plain(0x0b),
        "Minus" => plain(0x0c),
        "Equal" => plain(0x0d),
        "Backspace" => plain(0x0e),
        "Tab" => plain(0x0f),
        "KeyQ" => plain(0x10),
        "KeyW" => plain(0x11),
        "KeyE" => plain(0x12),
        "KeyR" => plain(0x13),
        "KeyT" => plain(0x14),
        "KeyY" => plain(0x15),
        "KeyU" => plain(0x16),
        "KeyI" => plain(0x17),
        "KeyO" => plain(0x18),
        "KeyP" => plain(0x19),
        "BracketLeft" => plain(0x1a),
        "BracketRight" => plain(0x1b),
        "Enter" => plain(0x1c),
        "ControlLeft" => plain(0x1d),
        "KeyA" => plain(0x1e),
        "KeyS" => plain(0x1f),
        "KeyD" => plain(0x20),
        "KeyF" => plain(0x21),
        "KeyG" => plain(0x22),
        "KeyH" => plain(0x23),
        "KeyJ" => plain(0x24),
        "KeyK" => plain(0x25),
        "KeyL" => plain(0x26),
        "Semicolon" => plain(0x27),
        "Quote" => plain(0x28),
        "Backquote" => plain(0x29),
        "ShiftLeft" => plain(0x2a),
        "Backslash" => plain(0x2b),
        "KeyZ" => plain(0x2c),
        "KeyX" => plain(0x2d),
        "KeyC" => plain(0x2e),
        "KeyV" => plain(0x2f),
        "KeyB" => plain(0x30),
        "KeyN" => plain(0x31),
        "KeyM" => plain(0x32),
        "Comma" => plain(0x33),
        "Period" => plain(0x34),
        "Slash" => plain(0x35),
        "ShiftRight" => plain(0x36),
        "NumpadMultiply" => plain(0x37),
        "AltLeft" => plain(0x38),
        "Space" => plain(0x39),
        "CapsLock" => plain(0x3a),
        "F1" => plain(0x3b),
        "F2" => plain(0x3c),
        "F3" => plain(0x3d),
        "F4" => plain(0x3e),
        "F5" => plain(0x3f),
        "F6" => plain(0x40),
        "F7" => plain(0x41),
        "F8" => plain(0x42),
        "F9" => plain(0x43),
        "F10" => plain(0x44),
        "ScrollLock" => plain(0x46),
        "Numpad7" => plain(0x47),
        "Numpad8" => plain(0x48),
        "Numpad9" => plain(0x49),
        "NumpadSubtract" => plain(0x4a),
        "Numpad4" => plain(0x4b),
        "Numpad5" => plain(0x4c),
        "Numpad6" => plain(0x4d),
        "NumpadAdd" => plain(0x4e),
        "Numpad1" => plain(0x4f),
        "Numpad2" => plain(0x50),
        "Numpad3" => plain(0x51),
        "Numpad0" => plain(0x52),
        "NumpadDecimal" => plain(0x53),
        "IntlBackslash" => plain(0x56),
        "F11" => plain(0x57),
        "F12" => plain(0x58),

        "NumpadEnter" => extended(0x1c),
        "ControlRight" => extended(0x1d),
        "NumpadDivide" => extended(0x35),
        "PrintScreen" => extended(0x37),
        "AltRight" => extended(0x38),
        "NumLock" => extended(0x45),
        "Home" => extended(0x47),
        "ArrowUp" => extended(0x48),
        "PageUp" => extended(0x49),
        "ArrowLeft" => extended(0x4b),
        "ArrowRight" => extended(0x4d),
        "End" => extended(0x4f),
        "ArrowDown" => extended(0x50),
        "PageDown" => extended(0x51),
        "Insert" => extended(0x52),
        "Delete" => extended(0x53),
        "MetaLeft" => extended(0x5b),
        "MetaRight" => extended(0x5c),
        "ContextMenu" => extended(0x5d),
        _ => None,
    }
}

/// The key code for a key named as browsers name it. Like a scan code, it describes a position on the
/// keyboard, and the sharer's layout decides what the key means.
#[cfg(target_os = "macos")]
fn key(code: &str) -> Option<Key> {
    let number = match code {
        "KeyA" => 0x00,
        "KeyS" => 0x01,
        "KeyD" => 0x02,
        "KeyF" => 0x03,
        "KeyH" => 0x04,
        "KeyG" => 0x05,
        "KeyZ" => 0x06,
        "KeyX" => 0x07,
        "KeyC" => 0x08,
        "KeyV" => 0x09,
        "IntlBackslash" => 0x0a,
        "KeyB" => 0x0b,
        "KeyQ" => 0x0c,
        "KeyW" => 0x0d,
        "KeyE" => 0x0e,
        "KeyR" => 0x0f,
        "KeyY" => 0x10,
        "KeyT" => 0x11,
        "Digit1" => 0x12,
        "Digit2" => 0x13,
        "Digit3" => 0x14,
        "Digit4" => 0x15,
        "Digit6" => 0x16,
        "Digit5" => 0x17,
        "Equal" => 0x18,
        "Digit9" => 0x19,
        "Digit7" => 0x1a,
        "Minus" => 0x1b,
        "Digit8" => 0x1c,
        "Digit0" => 0x1d,
        "BracketRight" => 0x1e,
        "KeyO" => 0x1f,
        "KeyU" => 0x20,
        "BracketLeft" => 0x21,
        "KeyI" => 0x22,
        "KeyP" => 0x23,
        "Enter" => 0x24,
        "KeyL" => 0x25,
        "KeyJ" => 0x26,
        "Quote" => 0x27,
        "KeyK" => 0x28,
        "Semicolon" => 0x29,
        "Backslash" => 0x2a,
        "Comma" => 0x2b,
        "Slash" => 0x2c,
        "KeyN" => 0x2d,
        "KeyM" => 0x2e,
        "Period" => 0x2f,
        "Tab" => 0x30,
        "Space" => 0x31,
        "Backquote" => 0x32,
        "Backspace" => 0x33,
        "Escape" => 0x35,
        "MetaRight" => 0x36,
        "MetaLeft" => 0x37,
        "ShiftLeft" => 0x38,
        "CapsLock" => 0x39,
        "AltLeft" => 0x3a,
        "ControlLeft" => 0x3b,
        "ShiftRight" => 0x3c,
        "AltRight" => 0x3d,
        "ControlRight" => 0x3e,
        "NumpadDecimal" => 0x41,
        "NumpadMultiply" => 0x43,
        "NumpadAdd" => 0x45,
        "NumLock" => 0x47,
        "NumpadDivide" => 0x4b,
        "NumpadEnter" => 0x4c,
        "NumpadSubtract" => 0x4e,
        "NumpadEqual" => 0x51,
        "Numpad0" => 0x52,
        "Numpad1" => 0x53,
        "Numpad2" => 0x54,
        "Numpad3" => 0x55,
        "Numpad4" => 0x56,
        "Numpad5" => 0x57,
        "Numpad6" => 0x58,
        "Numpad7" => 0x59,
        "Numpad8" => 0x5b,
        "Numpad9" => 0x5c,
        "F5" => 0x60,
        "F6" => 0x61,
        "F7" => 0x62,
        "F3" => 0x63,
        "F8" => 0x64,
        "F9" => 0x65,
        "F11" => 0x67,
        "PrintScreen" => 0x69,
        "ScrollLock" => 0x6b,
        "F10" => 0x6d,
        "ContextMenu" => 0x6e,
        "F12" => 0x6f,
        "Pause" => 0x71,
        "Insert" => 0x72,
        "Home" => 0x73,
        "PageUp" => 0x74,
        "Delete" => 0x75,
        "F4" => 0x76,
        "End" => 0x77,
        "F2" => 0x78,
        "PageDown" => 0x79,
        "F1" => 0x7a,
        "ArrowLeft" => 0x7b,
        "ArrowRight" => 0x7c,
        "ArrowDown" => 0x7d,
        "ArrowUp" => 0x7e,
        _ => return None,
    };
    Some(Key(number))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn key(_code: &str) -> Option<Key> {
    None
}

/// Injects input and remembers what it is holding down, so that no button or key is left stuck when
/// control is taken back or the connection ends.
pub struct Injector {
    buttons: Vec<MouseButton>,
    keys: Vec<Key>,
    /// Running for as long as the helper has control.
    sharer: Option<SharerWatch>,
    /// Makes the on-screen indicator let the mouse through to what is beneath it, or take it again.
    let_through: Arc<dyn Fn(bool) + Send + Sync>,
    /// When the helper's pointer was last over the indicator, while it is still letting the mouse through.
    through_since: Option<Instant>,
    /// When the indicator was last told to let the mouse through.
    opened: Option<Instant>,
}

/// With `LIGHT_ASSIST_TRACE` set, says on standard error which of the app's windows a click was found
/// to be over, for when the helper reaches one they should not.
fn traced(over: Option<Own>) -> Option<Own> {
    if std::env::var_os("LIGHT_ASSIST_TRACE").is_some() {
        eprintln!("trace: a click or scroll over {}", match over {
            Some(Own::Console) => "the console (refused)",
            Some(Own::Indicator) => "the indicator (let through)",
            None => "none of the app's windows",
        });
    }
    over
}

/// One of this app's own windows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Own {
    /// The app's window, which the helper sees and must not be able to use.
    Console,
    /// The bar that stays above everything while a helper is connected, which the helper is not shown.
    Indicator,
}

/// How long after the helper's pointer was last over the indicator it stays out of the mouse's way:
/// long enough for a click to have arrived underneath, too short for the sharer to find the bar's own
/// buttons dead. The sharer's own mouse or keyboard ends it at once.
const THROUGH_FOR: Duration = Duration::from_millis(150);
/// How long macOS takes to act on a window being told to let the mouse through. Measured on an M4:
/// a click sent 15 ms after still landed on the window half the time, and one sent 40 ms after never did.
/// Windows acts on it at once.
const SETTLES_IN: Duration = Duration::from_millis(if cfg!(target_os = "macos") { 80 } else { 0 });

impl Injector {
    pub fn new(let_through: Arc<dyn Fn(bool) + Send + Sync>) -> Self {
        Self { buttons: Vec::new(), keys: Vec::new(), sharer: None, let_through, through_since: None, opened: None }
    }

    /// The helper cannot see the indicator, so a click where it happens to be is meant for what is
    /// beneath it. For that click the indicator steps out of the mouse's way.
    ///
    /// It starts doing so as soon as their pointer arrives over it, so that by the time they click
    /// the way is clear; `settled` is for a click or scroll, which waits out whatever is left of the
    /// time the system needs.
    fn reach_through(&mut self, settled: bool) {
        if self.through_since.is_none() {
            (self.let_through)(true);
            self.opened = Some(Instant::now());
        }
        self.through_since = Some(Instant::now());
        if let (true, Some(opened)) = (settled, self.opened) {
            std::thread::sleep(SETTLES_IN.saturating_sub(opened.elapsed()));
        }
    }

    /// Gives the indicator its buttons back, for the sharer, once the helper's click has gone by.
    /// At once when `now` is set, as when the sharer takes over.
    fn close_up(&mut self, now: bool) {
        let passed = self.through_since.is_some_and(|since| now || (self.buttons.is_empty() && since.elapsed() >= THROUGH_FOR));
        if passed {
            self.through_since = None;
            self.opened = None;
            (self.let_through)(false);
        }
    }

    /// The helper has control: start noticing the sharer's own input, if that has not started already.
    pub fn stand_by(&mut self) {
        self.sharer.get_or_insert_with(SharerWatch::start);
        // The bar's buttons are the sharer's: the moment they use their own mouse, they work.
        let sharer_active = self.sharer_active();
        self.close_up(sharer_active);
    }

    /// Whether the sharer is using their own mouse or keyboard, or did until a moment ago. While they
    /// are, the helper's input is set aside: the person sitting at the computer always comes first.
    pub fn sharer_active(&self) -> bool {
        self.sharer.as_ref().is_some_and(|sharer| sharer.sharer_active(SHARER_PRIORITY))
    }

    /// Injects one message from the helper, and returns whether it was. It is dropped while the sharer
    /// is active, and when it would land on Light Assist itself.
    pub fn apply(&mut self, message: InputMessage) -> bool {
        self.stand_by();
        if self.sharer_active() {
            // Whatever the helper was holding must not fight the sharer's own clicks and keys.
            self.release_all();
            return false;
        }

        // The helper must not be able to answer their own requests, so this app's windows are out of
        // their reach: no clicks or scrolling on them, and no typing while one of them has the keyboard.
        // Letting go of something already held is always allowed.
        let on_this_app = match &message {
            InputMessage::MouseDown { .. } | InputMessage::Wheel { .. } => match traced(os::pointer_on_this_app()) {
                Some(Own::Console) => true,
                Some(Own::Indicator) => {
                    self.reach_through(true);
                    false
                }
                None => false,
            },
            // The Windows key goes to Windows itself, never to the window that has the keyboard, and it
            // is how a helper gets away from the console, which is in front right after the sharer
            // clicks Allow there.
            InputMessage::KeyDown { code } if matches!(code.as_str(), "MetaLeft" | "MetaRight") => false,
            InputMessage::KeyDown { .. } | InputMessage::Text { .. } => os::keyboard_in_this_app(),
            InputMessage::MouseMove { .. } | InputMessage::MouseUp { .. } | InputMessage::KeyUp { .. } | InputMessage::Clipboard { .. } => false,
        };
        if on_this_app {
            return false;
        }

        match message {
            InputMessage::MouseMove { x, y } => {
                if x.is_finite() && y.is_finite() {
                    os::move_to(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0), shared_area());
                    // Arriving over the indicator, which they cannot see, is the first sign that a
                    // click may follow there.
                    if os::pointer_on_this_app() == Some(Own::Indicator) {
                        self.reach_through(false);
                    }
                }
            }
            InputMessage::MouseDown { button } => {
                if !self.buttons.contains(&button) {
                    self.buttons.push(button);
                    os::button(button, true);
                }
            }
            InputMessage::MouseUp { button } => {
                if let Some(index) = self.buttons.iter().position(|held| *held == button) {
                    self.buttons.swap_remove(index);
                    os::button(button, false);
                }
            }
            InputMessage::Wheel { dx, dy } => {
                if dx.is_finite() && dy.is_finite() {
                    os::wheel(dx.clamp(-10.0, 10.0), dy.clamp(-10.0, 10.0));
                }
            }
            InputMessage::KeyDown { code } => {
                if let Some(key) = key(&code) {
                    // A key held down repeats, so a second "down" for a held key is passed on as well.
                    if !self.keys.contains(&key) {
                        self.keys.push(key);
                    }
                    os::key(key, true);
                }
            }
            InputMessage::KeyUp { code } => {
                if let Some(index) = key(&code).and_then(|key| self.keys.iter().position(|held| *held == key)) {
                    let key = self.keys.swap_remove(index);
                    os::key(key, false);
                }
            }
            InputMessage::Text { text } => {
                for character in text.chars().take(MAX_TEXT_CHARS).filter(|character| !character.is_control()) {
                    os::character(character);
                }
            }
            InputMessage::Clipboard { text } => return crate::clipboard::set_text(&text),
        }
        true
    }

    /// The helper may no longer control this computer: let go of everything and stop watching the sharer.
    pub fn stand_down(&mut self) {
        self.release_all();
        self.sharer = None;
    }

    fn release_all(&mut self) {
        for button in self.buttons.drain(..) {
            os::button(button, false);
        }
        self.close_up(true);
        for key in self.keys.drain(..) {
            os::key(key, false);
        }
    }
}

impl Drop for Injector {
    fn drop(&mut self) {
        self.release_all();
    }
}

#[cfg(windows)]
mod os {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
    };

    use windows::Win32::{
        Foundation::{HWND, POINT},
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            GetAncestor, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowLongW, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT,
            GWL_EXSTYLE, WS_EX_TOPMOST,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    };

    use crate::protocol::MouseButton;

    /// One notch of a mouse wheel, as Windows counts it.
    const WHEEL_DELTA: f64 = 120.0;

    /// Whether `window` is part of one of this app's own windows. The page inside each of them is
    /// drawn by a separate browser process, so what counts is who owns the outermost window.
    unsafe fn belongs_to_this_app(window: HWND) -> bool {
        let outermost = GetAncestor(window, GA_ROOT);
        let mut owner = 0;
        GetWindowThreadProcessId(if outermost.is_invalid() { window } else { outermost }, Some(&mut owner));
        owner == GetCurrentProcessId()
    }

    /// Which of this app's windows the mouse pointer is over, if it is over one. The indicator is the
    /// one that stays above all others. While it lets the mouse through, Windows does not find it here
    /// at all, and answers with whatever is beneath.
    pub fn pointer_on_this_app() -> Option<super::Own> {
        unsafe {
            let mut position = POINT::default();
            GetCursorPos(&mut position).ok()?;
            let window = WindowFromPoint(position);
            if !belongs_to_this_app(window) {
                return None;
            }
            let outermost = GetAncestor(window, GA_ROOT);
            let topmost = GetWindowLongW(if outermost.is_invalid() { window } else { outermost }, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0;
            Some(if topmost { super::Own::Indicator } else { super::Own::Console })
        }
    }

    /// Whether the console or the indicator is the window being typed into.
    pub fn keyboard_in_this_app() -> bool {
        unsafe { belongs_to_this_app(GetForegroundWindow()) }
    }

    fn send(inputs: &[INPUT]) {
        let accepted = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };

        // Windows refuses input while the lock screen, a screen saver or an elevated window has the
        // desktop. Say so once instead of failing silently.
        static REPORTED: AtomicBool = AtomicBool::new(false);
        if accepted == 0 && !REPORTED.swap(true, Ordering::Relaxed) {
            eprintln!("Windows refused the helper's input: {}", std::io::Error::last_os_error());
        }
    }

    fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) {
        send(&[INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        }]);
    }

    fn keyboard(scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        }
    }

    /// `x` and `y` are fractions of the primary monitor, the one being shared. Absolute coordinates
    /// without the virtual-desktop flag address exactly that monitor, as 0 to 65535.
    /// `x` and `y` are fractions of `area`, the monitor being shared. Without an area they are
    /// fractions of the primary monitor.
    pub fn move_to(x: f64, y: f64, area: Option<(i32, i32, i32, i32)>) {
        let Some((left, top, width, height)) = area else {
            // Absolute coordinates without the virtual-desktop flag address the primary monitor, as 0 to 65535.
            return mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, (x * 65535.0).round() as i32, (y * 65535.0).round() as i32, 0);
        };

        // With the virtual-desktop flag, 0 to 65535 spans every monitor together.
        let (desk_left, desk_top, desk_width, desk_height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
            )
        };
        let pixel_x = left as f64 + x * (width - 1).max(0) as f64;
        let pixel_y = top as f64 + y * (height - 1).max(0) as f64;
        let to_desk = |pixel: f64, start: i32, span: i32| ((pixel - start as f64) * 65535.0 / (span - 1) as f64).round() as i32;
        mouse(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            to_desk(pixel_x, desk_left, desk_width),
            to_desk(pixel_y, desk_top, desk_height),
            0,
        );
    }

    pub fn button(button: MouseButton, down: bool) {
        let flags = match (button, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        };
        mouse(flags, 0, 0, 0);
    }

    /// `dx` and `dy` are in wheel notches, positive meaning right and down as in a browser. Windows
    /// counts a vertical turn the other way round.
    pub fn wheel(dx: f64, dy: f64) {
        if dy != 0.0 {
            mouse(MOUSEEVENTF_WHEEL, 0, 0, (-dy * WHEEL_DELTA).round() as i32);
        }
        if dx != 0.0 {
            mouse(MOUSEEVENTF_HWHEEL, 0, 0, (dx * WHEEL_DELTA).round() as i32);
        }
    }

    pub fn key(key: super::Key, down: bool) {
        let super::Key { scan, extended } = key;
        let mut flags = KEYEVENTF_SCANCODE;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        send(&[keyboard(scan, flags)]);
    }

    /// Types one character directly, bypassing the keyboard layout.
    pub fn character(character: char) {
        let mut units = [0u16; 2];
        for unit in character.encode_utf16(&mut units) {
            send(&[keyboard(*unit, KEYEVENTF_UNICODE), keyboard(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)]);
        }
    }
}

#[cfg(target_os = "macos")]
mod os {
    use std::{
        ffi::c_void,
        sync::Mutex,
        time::{Duration, Instant},
    };

    use crate::{
        mac::{windows_on_screen, CFRelease, CGPoint, CGRect},
        protocol::MouseButton,
        util::lock,
    };

    type CGEventRef = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreate(source: *const c_void) -> CGEventRef;
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        fn CGEventCreateMouseEvent(source: *const c_void, kind: u32, position: CGPoint, button: u32) -> CGEventRef;
        fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> CGEventRef;
        fn CGEventCreateScrollWheelEvent2(source: *const c_void, units: u32, wheels: u32, vertical: i32, horizontal: i32, third: i32) -> CGEventRef;
        fn CGEventSetType(event: CGEventRef, kind: u32);
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CGEventSetFlags(event: CGEventRef, flags: u64);
        fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
        fn CGEventKeyboardSetUnicodeString(event: CGEventRef, length: usize, units: *const u16);
        fn CGEventPost(tap: u32, event: CGEventRef);
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> CGRect;
    }

    // Kinds of event, as `CGEventType` numbers them.
    const LEFT_DOWN: u32 = 1;
    const LEFT_UP: u32 = 2;
    const RIGHT_DOWN: u32 = 3;
    const RIGHT_UP: u32 = 4;
    const MOVED: u32 = 5;
    const LEFT_DRAGGED: u32 = 6;
    const RIGHT_DRAGGED: u32 = 7;
    const FLAGS_CHANGED: u32 = 12;
    const OTHER_DOWN: u32 = 25;
    const OTHER_UP: u32 = 26;
    const OTHER_DRAGGED: u32 = 27;

    // Fields of an event, as `CGEventField` numbers them.
    const CLICK_COUNT: u32 = 1;
    const DELTA_X: u32 = 4;
    const DELTA_Y: u32 = 5;
    const USER_DATA: u32 = 42;

    /// Where events enter the system as if they came from the hardware.
    const HID_TAP: u32 = 0;
    /// Scrolling counted in pixels, which is what a helper's browser reports.
    const PIXELS: u32 = 0;
    const PIXELS_PER_NOTCH: f64 = 100.0;

    const SHIFT: u64 = 0x0002_0000;
    const CONTROL: u64 = 0x0004_0000;
    const OPTION: u64 = 0x0008_0000;
    const COMMAND: u64 = 0x0010_0000;
    const MODIFIERS: u64 = SHIFT | CONTROL | OPTION | COMMAND;

    /// Written into every event made here, so that the watch on the sharer's own mouse and keyboard
    /// can tell the helper's input from theirs. `sharer.rs` looks for the same number.
    const HELPERS_MARK: i64 = 0x4c41_4853;

    /// Two presses this close in time and place are a double click, which macOS wants to be told.
    const DOUBLE_CLICK: Duration = Duration::from_millis(500);
    const DOUBLE_CLICK_REACH: f64 = 4.0;
    const JUST_MOVED: Duration = Duration::from_millis(250);

    /// macOS does not work out for itself what a made-up event implies: which modifier keys are down,
    /// that a move with a button held is a drag, that a second press is a double click. So it is kept here.
    struct State {
        /// Where the helper last put the pointer, and when.
        position: Option<(CGPoint, Instant)>,
        buttons: Vec<MouseButton>,
        /// The modifier keys held, by key code.
        modifiers: Vec<u16>,
        last_press: Option<(MouseButton, Instant, CGPoint)>,
        clicks: i64,
    }

    static STATE: Mutex<State> = Mutex::new(State { position: None, buttons: Vec::new(), modifiers: Vec::new(), last_press: None, clicks: 1 });

    fn modifier(key: u16) -> u64 {
        match key {
            0x38 | 0x3c => SHIFT,
            0x3b | 0x3e => CONTROL,
            0x3a | 0x3d => OPTION,
            0x37 | 0x36 => COMMAND,
            _ => 0,
        }
    }

    impl State {
        fn flags(&self) -> u64 {
            self.modifiers.iter().fold(0, |flags, key| flags | modifier(*key))
        }

        /// Where the pointer is. A move the helper just made may not have arrived yet, so for a moment
        /// it is where they put it; after that it is wherever the pointer is on screen, which the sharer
        /// may have moved.
        fn place(&self) -> CGPoint {
            let recent = self.position.filter(|(_, at)| at.elapsed() < JUST_MOVED).map(|(place, _)| place);
            recent.unwrap_or_else(|| unsafe {
                let probe = CGEventCreate(std::ptr::null());
                if probe.is_null() {
                    return CGPoint::default();
                }
                let place = CGEventGetLocation(probe);
                CFRelease(probe);
                place
            })
        }
    }

    /// Sends an event on its way, with the modifier keys that are down. macOS drops it silently if
    /// the sharer has not let the app control the computer; `blocked.rs` tells the helper so.
    unsafe fn post(event: CGEventRef, flags: u64) {
        if event.is_null() {
            return;
        }
        CGEventSetFlags(event, (CGEventGetFlags(event) & !MODIFIERS) | flags);
        CGEventSetIntegerValueField(event, USER_DATA, HELPERS_MARK);
        CGEventPost(HID_TAP, event);
        CFRelease(event);
    }

    /// The windows a click or a key can land in, the one in front first: this app's own, whatever
    /// they float above, and other apps' ordinary ones. The system keeps windows of its own above
    /// everything, some as large as the screen and see-through (the Dock has one), which take no
    /// clicks and would hide every window behind them from this.
    fn windows_that_count() -> impl Iterator<Item = crate::mac::OnScreen> {
        let this_app = std::process::id() as i32;
        windows_on_screen().into_iter().filter(move |window| window.owner == this_app || window.layer == 0)
    }

    /// Which of this app's windows is in front at the pointer, if one is. The indicator is the one
    /// that floats above the others.
    pub fn pointer_on_this_app() -> Option<super::Own> {
        let place = lock(&STATE).place();
        let front = windows_that_count().find(|window| window.bounds.contains(place))?;
        (front.owner == std::process::id() as i32).then_some(if front.layer == 0 { super::Own::Console } else { super::Own::Indicator })
    }

    /// Whether this app is the one being typed into: macOS says it is in front, or its window is the
    /// frontmost of all. Either is enough, since being wrong the other way lets the helper type into it.
    pub fn keyboard_in_this_app() -> bool {
        let this_app = std::process::id() as i32;
        let active = objc2::rc::autoreleasepool(|_| {
            let front = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication();
            front.is_some_and(|app| app.processIdentifier() == this_app)
        });
        active || windows_that_count().find(|window| window.layer == 0).is_some_and(|window| window.owner == this_app)
    }

    /// `x` and `y` are fractions of `area`, the display being shared, or of the main display.
    pub fn move_to(x: f64, y: f64, area: Option<(i32, i32, i32, i32)>) {
        let (left, top, width, height) = area.map(|(left, top, width, height)| (left as f64, top as f64, width as f64, height as f64)).unwrap_or_else(|| unsafe {
            let main = CGDisplayBounds(CGMainDisplayID());
            (main.origin.x, main.origin.y, main.size.width, main.size.height)
        });
        let to = CGPoint { x: left + x * (width - 1.0).max(0.0), y: top + y * (height - 1.0).max(0.0) };

        let mut state = lock(&STATE);
        let from = state.place();
        state.position = Some((to, Instant::now()));
        // With a button down a move is a drag, and has to say which button.
        let (kind, button) = match state.buttons.first() {
            Some(MouseButton::Left) => (LEFT_DRAGGED, 0),
            Some(MouseButton::Right) => (RIGHT_DRAGGED, 1),
            Some(MouseButton::Middle) => (OTHER_DRAGGED, 2),
            None => (MOVED, 0),
        };
        unsafe {
            let event = CGEventCreateMouseEvent(std::ptr::null(), kind, to, button);
            if !event.is_null() {
                CGEventSetIntegerValueField(event, DELTA_X, (to.x - from.x).round() as i64);
                CGEventSetIntegerValueField(event, DELTA_Y, (to.y - from.y).round() as i64);
            }
            post(event, state.flags());
        }
    }

    pub fn button(button: MouseButton, down: bool) {
        let mut state = lock(&STATE);
        let place = state.place();
        if down {
            let again = state.last_press.is_some_and(|(last, at, there)| {
                last == button && at.elapsed() < DOUBLE_CLICK && (there.x - place.x).abs() <= DOUBLE_CLICK_REACH && (there.y - place.y).abs() <= DOUBLE_CLICK_REACH
            });
            state.clicks = if again { state.clicks + 1 } else { 1 };
            state.last_press = Some((button, Instant::now(), place));
            state.buttons.push(button);
        } else {
            state.buttons.retain(|held| *held != button);
        }
        let (kind, number) = match (button, down) {
            (MouseButton::Left, true) => (LEFT_DOWN, 0),
            (MouseButton::Left, false) => (LEFT_UP, 0),
            (MouseButton::Right, true) => (RIGHT_DOWN, 1),
            (MouseButton::Right, false) => (RIGHT_UP, 1),
            (MouseButton::Middle, true) => (OTHER_DOWN, 2),
            (MouseButton::Middle, false) => (OTHER_UP, 2),
        };
        unsafe {
            let event = CGEventCreateMouseEvent(std::ptr::null(), kind, place, number);
            if !event.is_null() {
                CGEventSetIntegerValueField(event, CLICK_COUNT, state.clicks);
            }
            post(event, state.flags());
        }
    }

    /// `dx` and `dy` are in wheel notches, positive meaning right and down as in a browser. macOS
    /// counts both the other way round.
    pub fn wheel(dx: f64, dy: f64) {
        let flags = lock(&STATE).flags();
        let pixels = |notches: f64| (-notches * PIXELS_PER_NOTCH).round() as i32;
        unsafe { post(CGEventCreateScrollWheelEvent2(std::ptr::null(), PIXELS, 2, pixels(dy), pixels(dx), 0), flags) }
    }

    pub fn key(key: super::Key, down: bool) {
        let mut state = lock(&STATE);
        if modifier(key.0) != 0 {
            state.modifiers.retain(|held| *held != key.0);
            if down {
                state.modifiers.push(key.0);
            }
        }
        unsafe {
            let event = CGEventCreateKeyboardEvent(std::ptr::null(), key.0, down);
            // A modifier going down or up is not a key press to macOS but a change of flags, and
            // only as that does the system count the key as held.
            if modifier(key.0) != 0 && !event.is_null() {
                CGEventSetType(event, FLAGS_CHANGED);
            }
            post(event, state.flags())
        }
    }

    /// Types one character directly, bypassing the keyboard layout and whatever modifier is down.
    pub fn character(character: char) {
        let mut units = [0u16; 2];
        let units = character.encode_utf16(&mut units);
        for down in [true, false] {
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), 0, down);
                if !event.is_null() {
                    CGEventKeyboardSetUnicodeString(event, units.len(), units.as_ptr());
                }
                post(event, 0);
            }
        }
    }
}

/// Only Windows and macOS can be controlled so far.
#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    use crate::protocol::MouseButton;

    pub fn pointer_on_this_app() -> Option<super::Own> {
        None
    }

    pub fn keyboard_in_this_app() -> bool {
        false
    }

    pub fn move_to(_x: f64, _y: f64, _area: Option<(i32, i32, i32, i32)>) {}
    pub fn button(_button: MouseButton, _down: bool) {}
    pub fn wheel(_dx: f64, _dy: f64) {}
    pub fn key(_key: super::Key, _down: bool) {}
    pub fn character(_character: char) {}
}
