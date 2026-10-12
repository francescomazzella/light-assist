//! The few things the app says outside its pages (the tray's menu, a tooltip, a reason given to the
//! system), in the language of the person at this computer. The pages have their own translations
//! (`apps/ui/src/locales`); what is said here is short enough to be written out where it is said.

use std::sync::OnceLock;

/// Whether this computer's user reads Italian before English, as the system has it.
fn italian() -> bool {
    static ITALIAN: OnceLock<bool> = OnceLock::new();
    *ITALIAN.get_or_init(|| os::language().to_ascii_lowercase().starts_with("it"))
}

/// `english`, or `italian` for a reader of Italian.
pub fn tr(english: &'static str, in_italian: &'static str) -> &'static str {
    if italian() {
        in_italian
    } else {
        english
    }
}

#[cfg(windows)]
mod os {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }

    /// The language Windows shows its own menus in, as a two-letter code where it is one the app has.
    pub fn language() -> String {
        // The low ten bits name the language; 0x10 is Italian, wherever it is spoken.
        match unsafe { GetUserDefaultUILanguage() } & 0x3ff {
            0x10 => "it",
            _ => "en",
        }
        .to_owned()
    }
}

#[cfg(target_os = "macos")]
mod os {
    use crate::mac::{text, CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef, Cf};

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> CFArrayRef;
    }

    /// The first of the languages the user has chosen in System Settings, such as `it-IT`.
    pub fn language() -> String {
        unsafe {
            let Some(languages) = Cf::owned(CFLocaleCopyPreferredLanguages()) else { return String::new() };
            if CFArrayGetCount(languages.0) == 0 {
                return String::new();
            }
            text(CFArrayGetValueAtIndex(languages.0, 0)).unwrap_or_default()
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    pub fn language() -> String {
        std::env::var("LANG").unwrap_or_default()
    }
}
