//! The sharer's preferences, kept in a small file next to the app's other data.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A day; nothing longer makes sense for a help session.
const MAX_MINUTES: u32 = 24 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// End the session when a helper with control or a terminal has done nothing for this long. 0 is never.
    pub idle_minutes: u32,
    /// End any session after this long. 0 is never.
    pub max_minutes: u32,
    /// Share copied text, both ways, with a helper who has control.
    pub clipboard: bool,
    /// Let trusted devices on the local network connect and take control while nobody is here.
    pub unattended: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Short by default: a help session is usually a few minutes, and a forgotten one is the risk.
        Self { idle_minutes: 5, max_minutes: 30, clipboard: true, unattended: false }
    }
}

/// Where the app keeps what it remembers. `LIGHT_ASSIST_DATA_DIR` puts it somewhere else, so that
/// a copy run for development or by the test scripts leaves the real app's settings alone.
pub fn data_folder() -> Option<PathBuf> {
    if let Some(elsewhere) = std::env::var_os("LIGHT_ASSIST_DATA_DIR").filter(|folder| !folder.is_empty()) {
        return Some(PathBuf::from(elsewhere));
    }
    let folder = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?
    };
    Some(folder.join("light-assist"))
}

fn file() -> Option<PathBuf> {
    Some(data_folder()?.join("settings.json"))
}

impl Settings {
    /// What was saved, or the defaults if nothing usable was.
    pub fn load() -> Self {
        file()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .unwrap_or_default()
            .sane()
    }

    pub fn sane(self) -> Self {
        Self { idle_minutes: self.idle_minutes.min(MAX_MINUTES), max_minutes: self.max_minutes.min(MAX_MINUTES), ..self }
    }

    pub fn save(&self) {
        let Some(path) = file() else { return };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, serde_json::to_vec_pretty(self).unwrap_or_default()));
        if let Err(error) = written {
            eprintln!("Settings could not be saved: {error}");
        }
    }
}
