//! Text copied on one computer, ready to paste on the other.
//!
//! Only while the helper has control, and only text. What the helper pastes is put on the sharer's
//! clipboard just before the paste; what is copied on the sharer's computer is sent to the helper
//! page. Something a password manager marked as a secret stays where it is.

use std::{
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

/// The most text that travels either way, in bytes of UTF-8. Larger than anyone copies by hand, and
/// small enough for one message on a data channel.
pub const MAX_TEXT_BYTES: usize = 100_000;
/// How often the sharer's clipboard is looked at.
const CHECK_EVERY: Duration = Duration::from_millis(200);

/// The clipboard's change count right after this app last wrote to it, so that the helper's own
/// text is not sent straight back to them.
static OWN_CHANGE: AtomicI64 = AtomicI64::new(-1);

/// Puts `text` on the sharer's clipboard, and reports whether it did.
pub fn set_text(text: &str) -> bool {
    if text.len() > MAX_TEXT_BYTES || !os::write(text) {
        return false;
    }
    OWN_CHANGE.store(os::change_count(), Ordering::Relaxed);
    true
}

/// What the sharer's clipboard came to hold.
#[derive(Debug, PartialEq, Eq)]
pub enum Copied {
    Text(String),
    /// More text than is sent.
    TooLarge,
}

/// Follows the sharer's clipboard for one helper.
#[derive(Default)]
pub struct Watch {
    /// The change count last seen; `None` until the first look, which only takes note of it.
    seen: Option<i64>,
    checked: Option<Instant>,
}

impl Watch {
    /// Stops following, so that nothing copied in the meantime is sent when following starts again.
    pub fn forget(&mut self) {
        self.seen = None;
    }

    /// The text copied since the last look, if any was.
    pub fn changed(&mut self) -> Option<Copied> {
        if self.checked.is_some_and(|checked| checked.elapsed() < CHECK_EVERY) {
            return None;
        }
        self.checked = Some(Instant::now());

        let count = os::change_count();
        if self.seen.replace(count).is_none_or(|seen| seen == count) || count == OWN_CHANGE.load(Ordering::Relaxed) {
            return None;
        }
        let text = os::read()?;
        if text.is_empty() {
            return None;
        }
        Some(if text.len() > MAX_TEXT_BYTES { Copied::TooLarge } else { Copied::Text(text) })
    }
}

#[cfg(windows)]
mod os {
    use windows::{
        core::w,
        Win32::{
            Foundation::{GlobalFree, HANDLE, HGLOBAL},
            System::{
                DataExchange::{
                    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
                    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
                },
                Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
            },
        },
    };

    /// `CF_UNICODETEXT`: UTF-16 text ending in a zero.
    const UNICODE_TEXT: u32 = 13;

    pub fn change_count() -> i64 {
        unsafe { GetClipboardSequenceNumber() as i64 }
    }

    /// The clipboard belongs to one program at a time; another may hold it for a moment.
    unsafe fn open() -> bool {
        for _ in 0..10 {
            if OpenClipboard(None).is_ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    pub fn read() -> Option<String> {
        unsafe {
            // Password managers ask clipboard watchers to leave what they copy alone.
            let secret = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
            if (secret != 0 && IsClipboardFormatAvailable(secret).is_ok()) || IsClipboardFormatAvailable(UNICODE_TEXT).is_err() || !open() {
                return None;
            }
            let text = GetClipboardData(UNICODE_TEXT).ok().and_then(|data| {
                let memory = HGLOBAL(data.0);
                let units = GlobalLock(memory) as *const u16;
                if units.is_null() {
                    return None;
                }
                let available = GlobalSize(memory) / 2;
                let all = std::slice::from_raw_parts(units, available);
                let length = all.iter().position(|unit| *unit == 0).unwrap_or(available);
                let text = String::from_utf16_lossy(&all[..length]);
                let _ = GlobalUnlock(memory);
                Some(text)
            });
            let _ = CloseClipboard();
            text
        }
    }

    pub fn write(text: &str) -> bool {
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        unsafe {
            let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) else { return false };
            let target = GlobalLock(memory) as *mut u16;
            if target.is_null() || !open() {
                let _ = GlobalFree(Some(memory));
                return false;
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), target, units.len());
            let _ = GlobalUnlock(memory);

            // Once handed over, the memory is the clipboard's.
            let written = EmptyClipboard().is_ok() && SetClipboardData(UNICODE_TEXT, Some(HANDLE(memory.0))).is_ok();
            if !written {
                let _ = GlobalFree(Some(memory));
            }
            let _ = CloseClipboard();
            written
        }
    }
}

#[cfg(target_os = "macos")]
mod os {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    /// What password managers mark a secret with (nspasteboard.org), and what they mark something
    /// that is only passing through with.
    const LEFT_ALONE: [&str; 2] = ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType"];

    pub fn change_count() -> i64 {
        NSPasteboard::generalPasteboard().changeCount() as i64
    }

    pub fn read() -> Option<String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let types = pasteboard.types()?;
        if types.iter().any(|kind| LEFT_ALONE.contains(&kind.to_string().as_str())) {
            return None;
        }
        pasteboard.stringForType(unsafe { NSPasteboardTypeString }).map(|text| text.to_string())
    }

    pub fn write(text: &str) -> bool {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
    }
}

/// Only Windows and macOS have a clipboard to share so far.
#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    pub fn change_count() -> i64 {
        0
    }

    pub fn read() -> Option<String> {
        None
    }

    pub fn write(_text: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the real clipboard of the computer running the tests, and puts back the text it held.
    /// Left out of an ordinary test run for that reason: `cargo test -- --ignored clipboard`.
    #[test]
    #[ignore]
    fn text_goes_onto_the_clipboard_and_is_not_sent_back() {
        let before = os::read();
        let mut watch = Watch::default();
        assert_eq!(watch.changed(), None, "the first look only takes note");

        assert!(set_text("Light Assist – ciao 👋"));
        assert_eq!(os::read().as_deref(), Some("Light Assist – ciao 👋"));
        watch.checked = None;
        assert_eq!(watch.changed(), None, "what this app wrote is not news");

        assert!(os::write("copied by the sharer"));
        watch.checked = None;
        assert_eq!(watch.changed(), Some(Copied::Text("copied by the sharer".to_owned())));

        assert!(!set_text(&"x".repeat(MAX_TEXT_BYTES + 1)));
        assert!(os::write(&"x".repeat(MAX_TEXT_BYTES + 1)));
        watch.checked = None;
        assert_eq!(watch.changed(), Some(Copied::TooLarge));

        os::write(before.as_deref().unwrap_or(""));
    }
}
