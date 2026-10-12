//! The sharer's video: one of their monitors, captured and encoded as H.264 for the helper.
//!
//! The capture is rebuilt, not stopped, whenever what it was set up for changes: the monitor's
//! resolution, HDR being switched on or off, the helper choosing another monitor, or a monitor being
//! unplugged. While the screen cannot be captured at all (the computer is locked, say) it keeps trying.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[cfg(windows)]
use crate::quality::Content;
use crate::quality::Target;

#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod encoder;
#[cfg(windows)]
mod hdr;
#[cfg(windows)]
mod monitors;

#[cfg(target_os = "macos")]
mod mac_capture;
#[cfg(target_os = "macos")]
mod mac_encoder;
#[cfg(target_os = "macos")]
mod mac_monitors;

/// The connection sends frames as they arrive, so a backlog only ever means it has stalled.
const MAX_QUEUED_EVENTS: usize = 4;
/// How often the monitors and HDR mode are looked at again: once a second.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// How long to wait before trying again when the screen cannot be captured.
const RETRY_AFTER: Duration = Duration::from_secs(2);

/// One encoded picture in Annex B form, stamped with the time since the source started.
pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub at: Duration,
}

/// A monitor the helper can choose to see.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub id: String,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

pub enum VideoEvent {
    Frame(EncodedFrame),
    /// The monitors there are, and the one being shown. Sent again whenever either changes.
    Screens { screens: Vec<ScreenInfo>, current: String },
    /// Whether the screen can be captured at the moment.
    Available(bool),
    /// The encoder in use, which changes only when the capture is rebuilt.
    Encoder(String),
}

/// Owns the capture and encoder thread; dropping it stops capturing.
pub struct VideoSource {
    events: mpsc::Receiver<VideoEvent>,
    wants_keyframe: Arc<AtomicBool>,
    cursor: Arc<AtomicBool>,
    chosen: Arc<Mutex<Option<String>>>,
    target: Arc<Mutex<Target>>,
    busy: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
}

/// What the capture thread shares with the connection.
struct Shared {
    wants_keyframe: Arc<AtomicBool>,
    cursor: Arc<AtomicBool>,
    chosen: Arc<Mutex<Option<String>>>,
    /// What to produce, as the quality controller last decided.
    target: Arc<Mutex<Target>>,
    /// How much of each frame's time capture and encoding take, in thousandths, averaged.
    busy: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
}

impl Shared {
    fn chosen(&self) -> Option<String> {
        self.chosen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    fn target(&self) -> Target {
        *self.target.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl VideoSource {
    /// Starts capturing, at `target` until told otherwise.
    pub fn screen(target: Target) -> Self {
        let (tx, events) = mpsc::channel(MAX_QUEUED_EVENTS);
        let source = Self {
            events,
            wants_keyframe: Arc::new(AtomicBool::new(false)),
            cursor: Arc::new(AtomicBool::new(true)),
            chosen: Arc::new(Mutex::new(None)),
            target: Arc::new(Mutex::new(target)),
            busy: Arc::new(AtomicU32::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let shared = Shared {
            wants_keyframe: source.wants_keyframe.clone(),
            cursor: source.cursor.clone(),
            chosen: source.chosen.clone(),
            target: source.target.clone(),
            busy: source.busy.clone(),
            stop: source.stop.clone(),
        };
        std::thread::spawn(move || {
            if let Err(error) = run(tx, &shared) {
                eprintln!("Screen sharing stopped: {error}");
            }
        });
        source
    }

    /// `None` once capturing has stopped for good.
    pub async fn next(&mut self) -> Option<VideoEvent> {
        self.events.recv().await
    }

    /// Whether the sharer's mouse cursor is part of the picture. A helper who is controlling already
    /// sees their own pointer in the same place, so drawing it again would show two.
    pub fn show_cursor(&self, visible: bool) {
        self.cursor.store(visible, Ordering::Relaxed);
    }

    /// The helper's browser lost a packet or just connected and needs a frame it can start from.
    pub fn request_keyframe(&self) {
        self.wants_keyframe.store(true, Ordering::Relaxed);
    }

    /// What to produce from now on: the bitrate changes at once, a new size or rate rebuilds the capture.
    pub fn set_target(&self, target: Target) {
        *self.target.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = target;
    }

    /// How much of each frame's time capture and encoding take, 0 to 1 and beyond when falling behind.
    pub fn encoder_busy(&self) -> f32 {
        self.busy.load(Ordering::Relaxed) as f32 / 1000.0
    }

    /// Shows the monitor with this id from now on. An id that is not connected means the primary one.
    pub fn choose_screen(&self, id: String) {
        *self.chosen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(id);
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn run(tx: mpsc::Sender<VideoEvent>, shared: &Shared) -> windows::core::Result<()> {
    use std::time::Instant;

    use tokio::sync::mpsc::error::TrySendError;
    use windows::Win32::{
        Media::MediaFoundation::{MFStartup, MFSTARTUP_LITE, MF_VERSION},
        System::Com::CoIncrementMTAUsage,
    };

    use crate::input::set_shared_area;

    // COM and Media Foundation are started once and kept for the life of the process. Shutting them
    // down at the end of each session crashed the agent, most likely because Windows Graphics
    // Capture still had work in flight on its own threads.
    static STARTED: std::sync::OnceLock<windows::core::Result<()>> = std::sync::OnceLock::new();
    STARTED
        .get_or_init(|| unsafe {
            CoIncrementMTAUsage()?;
            MFStartup(MF_VERSION, MFSTARTUP_LITE)
        })
        .clone()?;

    let stopped = || shared.stop.load(Ordering::Relaxed);
    // `false` once the connection has gone, which ends the thread.
    let report = |event: VideoEvent| tx.blocking_send(event).is_ok();

    let start = Instant::now();
    let mut frame = 0u64;
    let mut told: Option<(Vec<monitors::Monitor>, String)> = None;
    let mut available: Option<bool> = None;

    while !stopped() {
        // Which monitor to show: the one chosen, if it is still connected, otherwise the primary one.
        let connected = monitors::list();
        let chosen = shared.chosen();
        let Some(monitor) = connected.iter().find(|monitor| Some(&monitor.id) == chosen.as_ref()).or(connected.first()).cloned() else {
            std::thread::sleep(RETRY_AFTER);
            continue;
        };
        if told.as_ref() != Some(&(connected.clone(), monitor.id.clone())) {
            let screens = connected
                .iter()
                .map(|each| ScreenInfo { id: each.id.clone(), width: each.width, height: each.height, primary: each.primary })
                .collect();
            if !report(VideoEvent::Screens { screens, current: monitor.id.clone() }) {
                return Ok(());
            }
            told = Some((connected.clone(), monitor.id.clone()));
        }

        let target = shared.target();
        let opened = capture::Screen::open(monitor.handle, (target.width, target.height), target.fps, target.content == Content::Text)
            .and_then(|screen| {
                let (width, height) = screen.size();
                Ok((screen, encoder::H264::open(width, height, target.fps, target.bitrate_bps)?))
            });
        let (mut screen, mut encoder) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                if available != Some(false) {
                    eprintln!("The screen cannot be captured, trying again: {error}");
                    available = Some(false);
                    if !report(VideoEvent::Available(false)) {
                        return Ok(());
                    }
                }
                std::thread::sleep(RETRY_AFTER);
                continue;
            }
        };
        if available != Some(true) {
            available = Some(true);
            if !report(VideoEvent::Available(true)) {
                return Ok(());
            }
        }
        if !report(VideoEvent::Encoder(encoder.name.clone())) {
            return Ok(());
        }
        set_shared_area(Some((monitor.left, monitor.top, monitor.width, monitor.height)));

        let (width, height) = screen.size();
        // NV12: a full-size Y plane, then a half-height interleaved UV plane. Black until the first capture.
        let mut pixels = vec![16u8; (width * height * 3 / 2) as usize];
        pixels[(width * height) as usize..].fill(128);
        let mut cursor_shown = true;
        // Pictures are paced from here, so that time spent setting up is not made up for with a burst.
        let (mut paced_from, mut first) = (Instant::now(), frame);
        let mut fps = target.fps.max(1);
        let mut interval = Duration::from_secs(1) / fps;
        let mut bitrate = (target.bitrate_bps, fps);
        let mut last_check = Instant::now();

        loop {
            let due = paced_from + interval * (frame - first) as u32;
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            if stopped() {
                return Ok(());
            }
            let began = Instant::now();

            // A new size or kind of content needs the capture set up again, and a keyframe; a new rate or
            // bitrate does not.
            let wanted = shared.target();
            if (wanted.width, wanted.height, wanted.content) != (target.width, target.height, target.content) {
                break;
            }
            if wanted.fps != fps {
                fps = wanted.fps.max(1);
                interval = Duration::from_secs(1) / fps;
                (paced_from, first) = (Instant::now(), frame);
            }
            if (wanted.bitrate_bps, fps) != bitrate {
                bitrate = (wanted.bitrate_bps, fps);
                // The encoder still counts frames at the rate it was opened with, so its bitrate is scaled to
                // spend the same bits a second at the rate pictures actually come.
                encoder.set_bitrate((wanted.bitrate_bps as u64 * target.fps.max(1) as u64 / fps as u64) as u32);
            }

            // Anything that makes this capture the wrong one ends it, and the outer loop sets up the right one.
            if shared.chosen().is_some_and(|id| id != monitor.id) && connected.iter().any(|each| Some(&each.id) == shared.chosen().as_ref()) {
                break;
            }
            if last_check.elapsed() >= CHECK_EVERY {
                last_check = Instant::now();
                if monitors::list() != connected || hdr::display_mode(monitor.handle).hdr != screen.is_hdr() {
                    break;
                }
            }

            if shared.cursor.load(Ordering::Relaxed) != cursor_shown {
                cursor_shown = !cursor_shown;
                screen.show_cursor(cursor_shown);
            }

            // Windows only hands over a picture when the screen changed; otherwise the last one is sent
            // again, which costs almost nothing to encode and keeps the stream alive.
            if screen.read_latest(&mut pixels).is_err() || screen.stale() {
                break;
            }
            let keyframe = shared.wants_keyframe.swap(false, Ordering::Relaxed);

            let mut closed = false;
            let encoded = encoder.encode(&pixels, frame, keyframe, |data| {
                match tx.try_send(VideoEvent::Frame(EncodedFrame { data, at: start.elapsed() })) {
                    Ok(()) => {}
                    // A dropped frame breaks every frame after it until the next keyframe.
                    Err(TrySendError::Full(_)) => shared.wants_keyframe.store(true, Ordering::Relaxed),
                    Err(TrySendError::Closed(_)) => closed = true,
                }
            });
            frame += 1;
            // How much of the frame's time this took, averaged over about a second.
            let share = (began.elapsed().as_secs_f32() / interval.as_secs_f32() * 1000.0) as u32;
            let average = shared.busy.load(Ordering::Relaxed);
            shared.busy.store((average * 29 + share) / 30, Ordering::Relaxed);
            if closed {
                return Ok(());
            }
            if encoded.is_err() {
                break;
            }
        }
        // A new encoder always starts with a keyframe, and the browser takes the new size from it.
    }
    Ok(())
}

/// The same loop as on Windows, around macOS's own capture and encoder. A picture is never copied
/// here: macOS hands each one over on the graphics card, and the encoder takes it from there.
#[cfg(target_os = "macos")]
fn run(tx: mpsc::Sender<VideoEvent>, shared: &Shared) -> Result<(), String> {
    use std::time::Instant;

    use tokio::sync::mpsc::error::TrySendError;

    use crate::input::set_shared_area;

    let stopped = || shared.stop.load(Ordering::Relaxed);
    // `false` once the connection has gone, which ends the thread.
    let report = |event: VideoEvent| tx.blocking_send(event).is_ok();

    let start = Instant::now();
    let mut told: Option<(Vec<mac_monitors::Monitor>, String)> = None;
    let mut available: Option<bool> = None;

    while !stopped() {
        // Which display to show: the one chosen, if it is still connected, otherwise the main one.
        let connected = mac_monitors::list();
        let chosen = shared.chosen();
        let Some(monitor) = connected.iter().find(|monitor| Some(&monitor.id) == chosen.as_ref()).or(connected.first()).cloned() else {
            std::thread::sleep(RETRY_AFTER);
            continue;
        };
        if told.as_ref() != Some(&(connected.clone(), monitor.id.clone())) {
            let screens = connected
                .iter()
                .map(|each| ScreenInfo { id: each.id.clone(), width: each.width, height: each.height, primary: each.primary })
                .collect();
            if !report(VideoEvent::Screens { screens, current: monitor.id.clone() }) {
                return Ok(());
            }
            told = Some((connected.clone(), monitor.id.clone()));
        }

        let target = shared.target();
        let mut cursor_shown = shared.cursor.load(Ordering::Relaxed);
        let opened = mac_capture::Screen::open(&monitor, (target.width, target.height), target.fps, cursor_shown).and_then(|screen| {
            let (width, height) = screen.size();
            Ok((screen, mac_encoder::H264::open(width, height, target.fps, target.bitrate_bps)?))
        });
        let (mut screen, mut encoder) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                if available != Some(false) {
                    eprintln!("The screen cannot be captured, trying again: {error}");
                    available = Some(false);
                    if !report(VideoEvent::Available(false)) {
                        return Ok(());
                    }
                }
                std::thread::sleep(RETRY_AFTER);
                continue;
            }
        };
        if available != Some(true) {
            available = Some(true);
            if !report(VideoEvent::Available(true)) {
                return Ok(());
            }
        }
        if !report(VideoEvent::Encoder(encoder.name.clone())) {
            return Ok(());
        }
        set_shared_area(Some((monitor.left, monitor.top, monitor.points_wide, monitor.points_high)));

        // Pictures are paced from here, so that time spent setting up is not made up for with a burst.
        let (mut paced_from, mut paced) = (Instant::now(), 0u32);
        let mut fps = target.fps.max(1);
        let mut interval = Duration::from_secs(1) / fps;
        let mut bitrate = target.bitrate_bps;
        let mut last_check = Instant::now();
        // A new encoder starts with a keyframe by itself.
        shared.wants_keyframe.store(false, Ordering::Relaxed);

        loop {
            std::thread::sleep((paced_from + interval * paced).saturating_duration_since(Instant::now()));
            paced += 1;
            if stopped() {
                return Ok(());
            }
            let began = Instant::now();

            // A new size needs the capture set up again, and a keyframe; a new rate or bitrate does not.
            let wanted = shared.target();
            if (wanted.width, wanted.height, wanted.content) != (target.width, target.height, target.content) {
                break;
            }
            if wanted.fps != fps {
                fps = wanted.fps.max(1);
                interval = Duration::from_secs(1) / fps;
                (paced_from, paced) = (Instant::now(), 1);
                screen.set_fps(fps);
            }
            if wanted.bitrate_bps != bitrate {
                bitrate = wanted.bitrate_bps;
                encoder.set_bitrate(bitrate);
            }

            // Anything that makes this capture the wrong one ends it, and the outer loop sets up the right one.
            if shared.chosen().is_some_and(|id| id != monitor.id) && connected.iter().any(|each| Some(&each.id) == shared.chosen().as_ref()) {
                break;
            }
            if last_check.elapsed() >= CHECK_EVERY {
                last_check = Instant::now();
                // A change of resolution shows as a display of another size.
                if mac_monitors::list() != connected {
                    break;
                }
                screen.leave_out_overlays();
            }
            match screen.stopped() {
                // The sharer chose to stop sharing in macOS's own menu bar: that is not to be undone here.
                Some((_, true)) => return Err("the sharer stopped it from the menu bar".to_owned()),
                Some((error, false)) => {
                    eprintln!("macOS stopped capturing the screen, starting again: {error}");
                    break;
                }
                None => {}
            }

            if shared.cursor.load(Ordering::Relaxed) != cursor_shown {
                cursor_shown = !cursor_shown;
                screen.show_cursor(cursor_shown);
            }

            // macOS only hands over a picture when the screen changed; otherwise the last one is sent
            // again, which costs almost nothing to encode and keeps the stream alive.
            let Some((picture, _)) = screen.latest() else { continue };
            let keyframe = shared.wants_keyframe.swap(false, Ordering::Relaxed);

            let mut closed = false;
            let encoded = encoder.encode(picture.0, start.elapsed(), keyframe, |data| {
                match tx.try_send(VideoEvent::Frame(EncodedFrame { data, at: start.elapsed() })) {
                    Ok(()) => {}
                    // A dropped frame breaks every frame after it until the next keyframe.
                    Err(TrySendError::Full(_)) => shared.wants_keyframe.store(true, Ordering::Relaxed),
                    Err(TrySendError::Closed(_)) => closed = true,
                }
            });
            // How much of the frame's time this took, averaged over about a second.
            let share = (began.elapsed().as_secs_f32() / interval.as_secs_f32() * 1000.0) as u32;
            let average = shared.busy.load(Ordering::Relaxed);
            shared.busy.store((average * 29 + share) / 30, Ordering::Relaxed);
            if closed {
                return Ok(());
            }
            if let Err(error) = encoded {
                eprintln!("Encoding stopped, starting again: {error}");
                break;
            }
        }
        // A new encoder always starts with a keyframe, and the browser takes the new size from it.
    }
    Ok(())
}

/// Elsewhere there is no screen sharing so far.
#[cfg(not(any(windows, target_os = "macos")))]
fn run(_tx: mpsc::Sender<VideoEvent>, _shared: &Shared) -> Result<(), &'static str> {
    let _ = (CHECK_EVERY, RETRY_AFTER);
    Err("screen sharing is not implemented on this platform")
}
