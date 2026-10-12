//! Noticing when the sharer's system keeps the helper out.
//!
//! On Windows two things do, and neither says so by itself. A permission prompt or the lock screen is shown on a
//! screen of its own, which can be neither captured nor typed into: the helper's picture simply stops.
//! And a window running as administrator ignores the mouse and keyboard of a program that is not.
//!
//! A Mac lets no program see the screen, or use the mouse and keyboard, until its user has allowed
//! that program to in System Settings. Until then the picture is missing, or input does nothing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blocked {
    /// Windows is showing a permission prompt or the lock screen.
    ProtectedScreen,
    /// The window in front runs as administrator.
    ElevatedWindow,
    /// The sharer's Mac has not been told to let the app show its screen.
    ScreenNotPermitted,
    /// The sharer's Mac has not been told to let the app use its mouse and keyboard.
    ControlNotPermitted,
}

#[cfg(windows)]
pub fn state() -> Option<Blocked> {
    use std::sync::OnceLock;

    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::{
            StationsAndDesktops::{CloseDesktop, OpenInputDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS},
            Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };

    /// Whether a process runs as administrator; `None` when Windows will not say, which it will not
    /// to a program with fewer rights than the one asked about.
    unsafe fn elevated(process: HANDLE) -> Option<bool> {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let (mut elevation, mut size) = (TOKEN_ELEVATION::default(), 0);
        let read = GetTokenInformation(token, TokenElevation, Some(&mut elevation as *mut _ as *mut _), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut size);
        let _ = CloseHandle(token);
        read.ok().map(|()| elevation.TokenIsElevated != 0)
    }

    unsafe {
        // The screen that takes the mouse and keyboard can only be opened while it is the ordinary one.
        match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) {
            Ok(desktop) => {
                let _ = CloseDesktop(desktop);
            }
            Err(_) => return Some(Blocked::ProtectedScreen),
        }

        // An agent that itself runs as administrator is kept out of nothing.
        static OURSELVES: OnceLock<bool> = OnceLock::new();
        if *OURSELVES.get_or_init(|| elevated(GetCurrentProcess()).unwrap_or(false)) {
            return None;
        }
        let window = GetForegroundWindow();
        let mut process_id = 0;
        if window.is_invalid() || GetWindowThreadProcessId(window, Some(&mut process_id)) == 0 || process_id == std::process::id() {
            return None;
        }
        let above_us = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
            Ok(process) => {
                let above = elevated(process).unwrap_or(true);
                let _ = CloseHandle(process);
                above
            }
            Err(_) => true,
        };
        above_us.then_some(Blocked::ElevatedWindow)
    }
}

#[cfg(target_os = "macos")]
pub fn state() -> Option<Blocked> {
    use crate::mac;

    if !mac::may_record_screen() {
        Some(Blocked::ScreenNotPermitted)
    } else if mac::screen_locked() {
        Some(Blocked::ProtectedScreen)
    } else if !mac::may_control() {
        Some(Blocked::ControlNotPermitted)
    } else {
        None
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn state() -> Option<Blocked> {
    None
}
