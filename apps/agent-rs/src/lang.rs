//! The few things the app says outside its pages (the tray's menu, a tooltip, a reason given to the
//! system), in the language of the person at this computer. The pages have their own translations
//! (`apps/ui/src/locales`); what is said here is short enough to be written out where it is said.

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    English,
    Italian,
    Spanish,
}

/// The first language of this computer's user, as the system has it, where the app has it.
fn language() -> Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    *LANGUAGE.get_or_init(|| match os::language().to_ascii_lowercase().get(..2) {
        Some("it") => Language::Italian,
        Some("es") => Language::Spanish,
        _ => Language::English,
    })
}

/// The text in the reader's language: `english`, `italian` or `spanish`.
pub fn tr(english: &'static str, italian: &'static str, spanish: &'static str) -> &'static str {
    match language() {
        Language::English => english,
        Language::Italian => italian,
        Language::Spanish => spanish,
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
        // The low ten bits name the language, wherever it is spoken: 0x10 is Italian, 0x0a Spanish.
        match unsafe { GetUserDefaultUILanguage() } & 0x3ff {
            0x10 => "it",
            0x0a => "es",
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
