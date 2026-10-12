//! The sharer's WebRTC endpoint (the helper's browser is the other peer).

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Once,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use str0m::{
    change::SdpOffer,
    channel::ChannelId,
    format::Codec,
    media::{Frequency, MediaKind, MediaTime, Mid, Pt},
    net::{Protocol, Receive},
    rtp::Extension,
    Candidate, Event, IceConnectionState, Input, Output, Rtc,
};
use tokio::{net::UdpSocket, task::JoinHandle};

use crate::audio::{AudioFrame, AudioSource};
use crate::quality::{Controller, Link, Observation, Preference, Target};
use crate::{
    input::Injector,
    protocol::{InputMessage, TerminalMessage},
    terminal::{Terminal, TerminalEvent},
    util::is_private_network,
    video::{EncodedFrame, ScreenInfo, VideoEvent, VideoSource},
};

const CONTROL_CHANNEL: &str = "control";
const INPUT_CHANNEL: &str = "input";
const TERMINAL_CHANNEL: &str = "terminal";
const MAX_CONTROL_BYTES: usize = 1024;
const BLOCKED_CHECK_EVERY: Duration = Duration::from_millis(500);
const MAX_INPUT_BYTES: usize = 256;
/// Room for the text the helper pastes, with what JSON adds around it. Browsers take and send up to this much in one message.
const MAX_CLIPBOARD_MESSAGE_BYTES: usize = 250_000;
/// Room for a pasted block of text.
const MAX_TERMINAL_BYTES: usize = 16 * 1024;

/// What the sharer currently allows this helper, asked afresh every time it matters.
pub struct Grants {
    /// The mouse and keyboard.
    pub control: Box<dyn Fn() -> bool + Send>,
    /// Copied text following from one computer to the other, which needs control and the sharer's setting.
    pub clipboard: Box<dyn Fn() -> bool + Send>,
    /// What this helper's browser is to be handed, once, because the sharer made it a trusted device.
    pub enrollment: Box<dyn Fn() -> Option<crate::devices::Enrollment> + Send>,
    /// A terminal on the sharer's computer.
    pub terminal: Box<dyn Fn() -> bool + Send>,
    /// Called when the helper actually does something with either, for the idle timeout.
    pub activity: Box<dyn Fn() + Send>,
    /// Makes the on-screen indicator let the mouse through to what is beneath it, or take it again.
    pub let_through: std::sync::Arc<dyn Fn(bool) + Send + Sync>,
    /// Called when the direct connection to the helper comes up, and when it has gone.
    pub linked: Box<dyn Fn(bool) + Send>,
    /// Called when the connection to the helper becomes slow, or recovers.
    pub slow: Box<dyn Fn(bool) + Send>,
}

/// Messages on the `control` data channel.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Control {
    /// To the helper: the connection is up, and what kind of computer the sharer's is (`windows`, `macos`).
    Hello {
        #[serde(default)]
        platform: String,
    },
    /// The screen cannot be captured, for instance because the sharer's computer is locked.
    ScreenUnavailable,
    /// It can be again.
    ScreenAvailable,
    /// The sharer's monitors, and the one being shown.
    Screens { screens: Vec<ScreenInfo>, current: String },
    /// Which encoder makes the picture: the graphics card's, by name, or the software one.
    Encoder { name: String },
    /// From the helper: show this monitor instead.
    SelectScreen { id: String },
    /// From the helper: their choices of size, rate and bitrate, each possibly automatic.
    QualityPreference(Preference),
    /// From the helper, every couple of seconds: their picture's size in pixels, and whether their
    /// browser is dropping pictures it cannot decode in time.
    Viewer { width: u32, height: u32, struggling: bool },
    /// To the helper: what is being sent, and whether the connection is slow.
    Quality { target: Target, slow: bool },
    /// The helper's input is being dropped, or accepted again, because the sharer is using the computer.
    Yielding { active: bool },
    /// The shape of the sharer's cursor, for the helper's own pointer to take.
    #[serde(skip_deserializing)]
    Cursor(crate::cursor::Shape),
    /// Windows is keeping the helper out, or no longer is.
    Blocked { reason: Option<crate::blocked::Blocked> },
    /// Text copied on the sharer's computer, for the helper's clipboard. Without text: it was too much to send.
    Clipboard { text: Option<String> },
    /// To the helper: the sharer trusts this browser. With this secret it may connect while nobody is
    /// at the sharer's computer. It travels here, on the encrypted connection, and nowhere else.
    Trust { id: String, secret: String, name: String },
    Ping { timestamp: f64 },
    Pong { timestamp: f64 },
}

/// The certificate the agent encrypts one helper's connection with, made before there is a
/// connection, because the pairing code is worked out from its fingerprint (`pairing.rs`).
pub struct Identity {
    certificate: str0m::config::DtlsCert,
    /// As SDP writes it: `AB:CD:…`.
    pub fingerprint: String,
}

impl Identity {
    pub fn new() -> Result<Self, String> {
        install_crypto();
        let certificate = str0m::crypto::from_feature_flags().dtls_provider.generate_certificate().ok_or("no certificate could be made")?;
        let digest = crate::devices::sha256(&certificate.certificate);
        let fingerprint = digest.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(":");
        Ok(Self { certificate, fingerprint })
    }
}

fn install_crypto() {
    static CRYPTO: Once = Once::new();
    CRYPTO.call_once(|| str0m::crypto::from_feature_flags().install_process_default());
}

/// Owns the connection task; dropping it tears the connection down.
pub struct RtcLink {
    task: JoinHandle<()>,
}

impl Drop for RtcLink {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The address of this computer's interface that routes towards `peer`.
pub fn local_address_towards(peer: Ipv4Addr) -> std::io::Result<Ipv4Addr> {
    let probe = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    probe.connect((peer, 9))?;
    match probe.local_addr()?.ip() {
        IpAddr::V4(address) => Ok(address),
        IpAddr::V6(_) => Err(std::io::Error::other("no IPv4 route to the helper")),
    }
}

/// Browsers may hide host candidates behind mDNS names, and a helper must not be able to make this
/// computer probe arbitrary hosts, so only literal LAN and Tailscale addresses survive. A hidden
/// candidate is learned from the browser's first connectivity check instead.
fn sanitize_offer(sdp: &str) -> String {
    let mut cleaned = String::with_capacity(sdp.len());
    for line in sdp.lines() {
        if let Some(candidate) = line.strip_prefix("a=candidate:") {
            let address = candidate.split_whitespace().nth(4).and_then(|text| text.parse::<Ipv4Addr>().ok());
            if !address.is_some_and(is_private_network) {
                continue;
            }
        }
        cleaned.push_str(line);
        cleaned.push_str("\r\n");
    }
    cleaned
}

/// This computer's addresses on the private networks it is on: its LAN, and Tailscale if it has it.
pub fn private_addresses() -> Vec<Ipv4Addr> {
    // The way to anywhere, and the way to Tailscale's own service address. No packet is sent.
    let mut found: Vec<Ipv4Addr> = [Ipv4Addr::new(192, 0, 2, 1), Ipv4Addr::new(100, 100, 100, 100)]
        .into_iter()
        .filter_map(|towards| local_address_towards(towards).ok())
        .filter(|address| is_private_network(*address))
        .collect();
    found.dedup();
    found
}

/// Accepts an SDP offer from the helper's browser and returns the SDP answer.
///
/// `helper` is the helper's address when their page connected to this computer directly, which says
/// which network to meet on. When the page came through the relay it is not known, and the helper is
/// offered every private network this computer is on.
///
/// The connection is made as `identity`, and only with the certificate whose fingerprint the helper
/// page gave when the pairing code was worked out: anyone else's is somebody in between.
pub async fn accept(offer: &str, helper: Option<Ipv4Addr>, identity: &Identity, expected: &str, grants: Grants) -> Result<(String, RtcLink), String> {
    install_crypto();
    if crate::pairing::fingerprint_in(offer).as_deref() != Some(expected) {
        return Err("the offer is not from the page that was paired with".to_owned());
    }

    let locals = match helper {
        Some(helper) => vec![local_address_towards(helper).map_err(|error| error.to_string())?],
        None => private_addresses(),
    };
    let mut sockets = Vec::with_capacity(locals.len());
    for local in locals {
        let socket = UdpSocket::bind((local, 0)).await.map_err(|error| error.to_string())?;
        let address = socket.local_addr().map_err(|error| error.to_string())?;
        sockets.push((socket, address));
    }
    if sockets.is_empty() {
        return Err("this computer is on no private network".to_owned());
    }

    let offer = SdpOffer::from_sdp_string(&sanitize_offer(offer)).map_err(|error| error.to_string())?;
    // H.264 only: it is what the encoder produces and what every browser decodes in hardware.
    // Bandwidth estimation measures how much the network towards the helper carries; statistics
    // report it once a second, along with loss.
    let mut rtc = Rtc::builder()
        .clear_codecs()
        .enable_h264(true)
        .enable_opus(true, false)
        // Lets the agent say how long the helper's browser may hold pictures before showing them.
        .set_extension(12, Extension::PlayoutDelay)
        .set_dtls_cert(identity.certificate.clone())
        .set_stats_interval(Some(Duration::from_secs(1)))
        .build(Instant::now());
    for (_, address) in &sockets {
        rtc.add_local_candidate(Candidate::host(*address, "udp").map_err(|error| error.to_string())?);
    }

    let answer = rtc.sdp_api().accept_offer(offer).map_err(|error| error.to_string())?;
    let task = tokio::spawn(drive(rtc, sockets, grants));
    Ok((answer.to_sdp_string(), RtcLink { task }))
}

fn send_control(rtc: &mut Rtc, channel: ChannelId, message: &Control) {
    if let (Some(mut channel), Ok(json)) = (rtc.channel(channel), serde_json::to_string(message)) {
        let _ = channel.write(false, json.as_bytes());
    }
}

/// The H.264 payload type to send on `mid`: constrained baseline in packetization mode 1 when the
/// browser offered it, which is what the encoder produces.
fn h264_payload(rtc: &mut Rtc, mid: Mid) -> Option<Pt> {
    let writer = rtc.writer(mid)?;
    let mut candidates: Vec<_> = writer
        .payload_params()
        .filter(|params| params.spec().codec == Codec::H264 && params.spec().format.packetization_mode == Some(1))
        .collect();
    candidates.sort_by_key(|params| params.spec().format.profile_level_id != Some(0x42e01f));
    candidates.first().map(|params| params.pt())
}

fn send_frame(rtc: &mut Rtc, mid: Mid, pt: Pt, frame: EncodedFrame, immediate: bool) {
    let time = MediaTime::new(frame.at.as_micros() as u64 * 90 / 1000, Frequency::NINETY_KHZ);
    // Shown the moment it arrives, or held for up to a second to even out the connection.
    let hold = MediaTime::new(if immediate { 0 } else { 100 }, Frequency::HUNDREDTHS);
    if let Some(writer) = rtc.writer(mid) {
        let _ = writer.playout_delay(MediaTime::new(0, Frequency::HUNDREDTHS), hold).write(pt, Instant::now(), time, frame.data);
    }
}

/// Tells the helper page that the shell is gone, whether it exited or the sharer closed the terminal.
fn send_terminal_closed(rtc: &mut Rtc, channel: Option<ChannelId>) {
    if let Some(mut channel) = channel.and_then(|channel| rtc.channel(channel)) {
        let _ = channel.write(false, br#"{"type":"closed"}"#);
    }
}

enum Step<'a> {
    Input(Option<Input<'a>>),
    Video(Option<VideoEvent>),
    Sound(Option<AudioFrame>),
    Shell(TerminalEvent),
}

/// Windows wakes sleepers up to 15 ms late unless asked for better, which is half a frame at 30 frames
/// a second and a whole one at 60: pictures would be captured and sent unevenly. Held for as long as a
/// helper is connected.
struct FineTimer;

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(period_ms: u32) -> u32;
    fn timeEndPeriod(period_ms: u32) -> u32;
}

impl FineTimer {
    fn new() -> Self {
        #[cfg(windows)]
        unsafe {
            timeBeginPeriod(1);
        }
        Self
    }
}

impl Drop for FineTimer {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            timeEndPeriod(1);
        }
    }
}

/// Keeps the sharer's computer and its screen awake while a helper is connected. A screen saver or
/// a sleeping computer would leave the helper with a picture that has stopped and input that does
/// nothing. The system shows the reason to anyone who asks what is keeping the computer up.
/// Kept as a number, which may move between threads where the handle itself may not.
struct Awake(Option<isize>);

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(kind: crate::mac::CFStringRef, level: u32, reason: crate::mac::CFStringRef, assertion: *mut u32) -> i32;
    fn IOPMAssertionRelease(assertion: u32) -> i32;
}

impl Awake {
    #[cfg(windows)]
    fn new() -> Self {
        use windows::{
            core::{w, PWSTR},
            Win32::System::{
                Power::{PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired, PowerSetRequest},
                Threading::{POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0},
            },
        };
        const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;
        let reason = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 { SimpleReasonString: PWSTR(w!("An assistant is connected to this computer").as_ptr() as *mut _) },
        };
        let request = unsafe { PowerCreateRequest(&reason) }.ok();
        if let Some(request) = request {
            unsafe {
                let _ = PowerSetRequest(request, PowerRequestDisplayRequired);
                let _ = PowerSetRequest(request, PowerRequestSystemRequired);
            }
        }
        Self(request.map(|request| request.0 as isize))
    }

    #[cfg(target_os = "macos")]
    fn new() -> Self {
        use crate::mac::Cf;
        /// `kIOPMAssertionLevelOn`
        const ON: u32 = 255;
        // Keeping the display on keeps the computer up as well.
        let (kind, reason) = (Cf::string("PreventUserIdleDisplaySleep"), Cf::string(crate::lang::tr("An assistant is connected to this computer", "Un assistente è connesso a questo computer")));
        let mut assertion = 0;
        let made = unsafe { IOPMAssertionCreateWithName(kind.0, ON, reason.0, &mut assertion) } == 0;
        Self(made.then_some(assertion as isize))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn new() -> Self {
        Self(None)
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        // Closing the request withdraws it.
        #[cfg(windows)]
        if let Some(request) = self.0.take() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(request as *mut _));
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(assertion) = self.0.take() {
            unsafe {
                IOPMAssertionRelease(assertion as u32);
            }
        }
    }
}

/// Ends a screen saver and turns the display back on, as touching the mouse would. A helper who
/// connects, above all one who connects while nobody is there, is shown the desktop and not the
/// screen saver, which takes no input from them. A screen saver set to ask for the password on the
/// way out leaves the lock screen instead, and that only the sharer can get past.
#[cfg(windows)]
fn wake_screen() {
    use windows::{
        core::{w, BOOL},
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            System::StationsAndDesktops::{CloseDesktop, EnumDesktopWindows, OpenDesktopW, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, DESKTOP_WRITEOBJECTS},
            UI::{
                Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT},
                WindowsAndMessaging::{
                    GetClassNameW, GetForegroundWindow, IsWindowVisible, PostMessageW, SystemParametersInfoW, SPI_GETSCREENSAVERRUNNING,
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_CLOSE,
                },
            },
        },
    };

    unsafe extern "system" fn close(window: HWND, _: LPARAM) -> BOOL {
        if IsWindowVisible(window).as_bool() {
            let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        true.into()
    }

    unsafe {
        let mut running = BOOL(0);
        let asked = SystemParametersInfoW(SPI_GETSCREENSAVERRUNNING, 0, Some(&mut running as *mut BOOL as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        if asked.is_ok() && running.as_bool() {
            // A screen saver that locks runs on a desktop of its own; closing its windows ends it.
            match OpenDesktopW(w!("Screen-saver"), DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS.0 | DESKTOP_WRITEOBJECTS.0) {
                Ok(desktop) => {
                    let _ = EnumDesktopWindows(Some(desktop), Some(close), LPARAM(0));
                    let _ = CloseDesktop(desktop);
                }
                // One that does not is simply the window in front. Only a screen saver's window is
                // closed, never whatever else may have come to the front in the meantime.
                Err(_) => {
                    let front = GetForegroundWindow();
                    let mut class = [0u16; 64];
                    let length = GetClassNameW(front, &mut class) as usize;
                    if String::from_utf16_lossy(&class[..length]) == "WindowsScreenSaverClass" {
                        let _ = PostMessageW(Some(front), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            }
        }
        // A mouse that moves nowhere turns a display back on that had gone dark to save power.
        let still = INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 } } };
        SendInput(&[still], std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "macos")]
fn wake_screen() {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionDeclareUserActivity(name: crate::mac::CFStringRef, kind: u32, assertion: *mut u32) -> i32;
    }
    /// `kIOPMUserActiveLocal`: someone is at the computer, which turns its display on.
    const AT_THE_COMPUTER: u32 = 0;

    let reason = crate::mac::Cf::string(crate::lang::tr("An assistant is connected to this computer", "Un assistente è connesso a questo computer"));
    let mut assertion = 0;
    if unsafe { IOPMAssertionDeclareUserActivity(reason.0, AT_THE_COMPUTER, &mut assertion) } == 0 {
        // It lasts as long as the display's own timer, and `Awake` takes over from it.
        unsafe { IOPMAssertionRelease(assertion) };
    }
    // The screen saver is a program of its own, which ends when asked to.
    objc2::rc::autoreleasepool(|_| {
        for saver in NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str("com.apple.ScreenSaver.Engine")) {
            saver.terminate();
        }
    });
}

#[cfg(not(any(windows, target_os = "macos")))]
fn wake_screen() {}

/// Waits for a packet on any of the sockets, and says which one it came in on.
async fn receive(sockets: &[(UdpSocket, SocketAddr)], buffer: &mut [u8]) -> (SocketAddr, std::io::Result<(usize, SocketAddr)>) {
    loop {
        let (_, index, _) = futures_util::future::select_all(sockets.iter().map(|(socket, _)| Box::pin(socket.readable()))).await;
        let (socket, local) = &sockets[index];
        match socket.try_recv_from(buffer) {
            // Readable a moment ago and not any more: wait again.
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            received => return (*local, received),
        }
    }
}

/// Says that the connection has gone when the task that drives it ends, however it ends.
struct Unlink(Box<dyn Fn(bool) + Send>);

impl Drop for Unlink {
    fn drop(&mut self) {
        (self.0)(false);
    }
}

async fn drive(mut rtc: Rtc, sockets: Vec<(UdpSocket, SocketAddr)>, mut grants: Grants) {
    let linked = Unlink(std::mem::replace(&mut grants.linked, Box::new(|_| {})));
    wake_screen();
    let _fine_timer = FineTimer::new();
    let _awake = Awake::new();
    let mut buffer = vec![0u8; 2048];
    let mut control: Option<ChannelId> = None;
    let mut input: Option<ChannelId> = None;
    let mut terminal: Option<ChannelId> = None;
    // Dropping the shell ends it, so it never outlives this connection or the terminal grant.
    let mut shell: Option<Terminal> = None;
    // Dropping the injector releases any button it holds, so none outlives this connection.
    let mut injector = Injector::new(grants.let_through.clone());
    let mut yielding = false;
    let mut cursors = crate::cursor::Cursors::default();
    let mut clipboard = crate::clipboard::Watch::default();
    // What the helper was last told about being kept out; unknown until the first look.
    let mut blocked: Option<Option<crate::blocked::Blocked>> = None;
    let mut blocked_checked = Instant::now();
    let mut video: Option<(Mid, Pt)> = None;
    // The computer's sound goes with its screen, for as long as the helper's browser takes it.
    let mut sound: Option<(Mid, Pt, AudioSource)> = None;
    // Dropping the source stops the encoder, so it lives exactly as long as this connection.
    let mut source: Option<VideoSource> = None;
    let mut screen_unavailable = false;
    // The last monitor list, for a control channel that opens after it was first sent.
    let mut screens: Option<Control> = None;
    let mut encoder_name: Option<Control> = None;
    // Created once the screen's size is known, from the first list of monitors.
    let mut quality: Option<Controller> = None;
    let mut preference = Preference::default();
    let mut viewer: Option<(u32, u32, bool)> = None;
    let mut slow = false;
    // Media bytes sent by the last stats event, to tell how much of the link the picture actually uses.
    let mut sent_before: Option<u64> = None;
    let mut link = Link::new();

    loop {
        // The connection only knows the time it is told, and stamps what it sends and measures with it.
        if rtc.handle_input(Input::Timeout(Instant::now())).is_err() {
            return;
        }
        // Drain everything the connection wants to do until it asks to be woken later.
        let wake = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(at)) => break at,
                Ok(Output::Transmit(packet)) => {
                    // From the address the helper is talking to, where this computer has more than one.
                    let socket = sockets.iter().find(|(_, local)| *local == packet.source).map_or(&sockets[0].0, |(socket, _)| socket);
                    let _ = socket.send_to(&packet.contents, packet.destination).await;
                }
                Ok(Output::Event(event)) => match event {
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => rtc.disconnect(),
                    // Once a second: decide size, rate and bitrate from what was measured.
                    Event::PeerStats(stats) => {
                        if let (Some(controller), Some(source)) = (quality.as_mut(), &source) {
                            let sent_bps = sent_before.replace(stats.bytes_tx).map(|before| stats.bytes_tx.saturating_sub(before) as f64 * 8.0);
                            let estimate = link.update(sent_bps, stats.egress_loss_fraction, stats.rtt.map(|rtt| rtt.as_secs_f64()));
                            let seen = Observation {
                                estimate_bps: Some(estimate),
                                sent_bps,
                                loss: stats.egress_loss_fraction,
                                viewer: viewer.map(|(width, height, _)| (width, height)),
                                viewer_struggling: viewer.is_some_and(|(_, _, struggling)| struggling),
                                encoder_busy: source.encoder_busy(),
                            };
                            let before = controller.target();
                            let target = controller.tick(&seen);
                            source.set_target(target);
                            // `LIGHT_ASSIST_PRETEND_SLOW` shows what a slow connection looks like to both
                            // people without having to find one.
                            let now_slow = controller.slow() || std::env::var_os("LIGHT_ASSIST_PRETEND_SLOW").is_some();
                            let was_slow = slow;
                            if now_slow != slow {
                                slow = now_slow;
                                (grants.slow)(slow);
                            }
                            if target != before || slow != was_slow {
                                if let Some(channel) = control {
                                    send_control(&mut rtc, channel, &Control::Quality { target, slow });
                                }
                            }
                        }
                    }
                    Event::MediaAdded(media) if media.kind == MediaKind::Video && video.is_none() => {
                        if let Some(pt) = h264_payload(&mut rtc, media.mid) {
                            video = Some((media.mid, pt));
                            // A sensible start until the screen's size and the network are known.
                            source = Some(VideoSource::screen(Controller::new((1920, 1080)).target()));
                        }
                    }
                    Event::MediaAdded(media) if media.kind == MediaKind::Audio && sound.is_none() => {
                        let opus = rtc.writer(media.mid).and_then(|writer| writer.payload_params().find(|params| params.spec().codec == Codec::Opus).map(|params| params.pt()));
                        if let Some(pt) = opus {
                            sound = Some((media.mid, pt, AudioSource::system()));
                        }
                    }
                    // Frames written before the connection is up are dropped, so start from a fresh keyframe.
                    event @ (Event::Connected | Event::KeyframeRequest(_)) => {
                        if matches!(event, Event::Connected) {
                            (linked.0)(true);
                        }
                        if let Some(source) = &source {
                            source.request_keyframe();
                        }
                    }
                    Event::ChannelOpen(channel, label) if label == CONTROL_CHANNEL => {
                        control = Some(channel);
                        send_control(&mut rtc, channel, &Control::Hello { platform: std::env::consts::OS.to_owned() });
                        if screen_unavailable {
                            send_control(&mut rtc, channel, &Control::ScreenUnavailable);
                        }
                        if let Some(encoder) = &encoder_name {
                            send_control(&mut rtc, channel, encoder);
                        }
                        if let Some(screens) = &screens {
                            send_control(&mut rtc, channel, screens);
                        }
                    }
                    Event::ChannelOpen(channel, label) if label == INPUT_CHANNEL => input = Some(channel),
                    Event::ChannelData(data) if Some(data.id) == input && !data.binary => {
                        // The access level is checked here, on the sharer's side, for every message.
                        if data.data.len() <= MAX_CLIPBOARD_MESSAGE_BYTES && (grants.control)() {
                            if let Ok(message) = serde_json::from_slice::<InputMessage>(&data.data) {
                                // Only pasted text may be long, and only where the sharer shares the clipboard.
                                let allowed = match message {
                                    InputMessage::Clipboard { .. } => (grants.clipboard)(),
                                    _ => data.data.len() <= MAX_INPUT_BYTES,
                                };
                                if allowed && injector.apply(message) {
                                    (grants.activity)();
                                }
                            }
                        }
                    }
                    Event::ChannelOpen(channel, label) if label == TERMINAL_CHANNEL => terminal = Some(channel),
                    Event::ChannelData(data) if Some(data.id) == terminal && !data.binary => {
                        // Like input, the terminal grant is checked on the sharer's side for every message.
                        if data.data.len() <= MAX_TERMINAL_BYTES && (grants.terminal)() {
                            match (serde_json::from_slice::<TerminalMessage>(&data.data), &shell) {
                                (Ok(TerminalMessage::Resize { cols, rows }), Some(shell)) => shell.resize(cols, rows),
                                // The first size the helper reports is what starts the shell.
                                (Ok(TerminalMessage::Resize { cols, rows }), None) => match Terminal::open(cols, rows) {
                                    Ok(opened) => shell = Some(opened),
                                    Err(error) => {
                                        eprintln!("The terminal could not be started: {error}");
                                        send_terminal_closed(&mut rtc, terminal);
                                    }
                                },
                                (Ok(TerminalMessage::Stdin { data }), Some(shell)) => {
                                    shell.write(data.as_bytes());
                                    (grants.activity)();
                                }
                                _ => {}
                            }
                        }
                    }
                    Event::ChannelData(data) if Some(data.id) == control && !data.binary => {
                        if data.data.len() <= MAX_CONTROL_BYTES {
                            match serde_json::from_slice(&data.data) {
                                Ok(Control::Ping { timestamp }) => send_control(&mut rtc, data.id, &Control::Pong { timestamp }),
                                // Which of the sharer's monitors to look at is the helper's to choose:
                                // the sharer has already agreed to their screen being seen.
                                Ok(Control::SelectScreen { id }) => {
                                    if let Some(source) = &source {
                                        source.choose_screen(id);
                                    }
                                }
                                // The helper's choices win over anything automatic.
                                Ok(Control::QualityPreference(chosen)) => {
                                    preference = chosen;
                                    if let (Some(quality), Some(source)) = (quality.as_mut(), &source) {
                                        quality.set_preference(chosen);
                                        source.set_target(quality.target());
                                        send_control(&mut rtc, data.id, &Control::Quality { target: quality.target(), slow });
                                    }
                                }
                                Ok(Control::Viewer { width, height, struggling }) => viewer = Some((width, height, struggling)),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                },
                Err(_) => return,
            }
        };

        if !rtc.is_alive() {
            return;
        }
        let controlling = (grants.control)();
        if controlling {
            injector.stand_by();
        } else {
            // Control was taken back, perhaps in the middle of a drag.
            injector.stand_down();
        }
        // Tell the helper why their input does nothing, and when it works again.
        let sharer_active = injector.sharer_active();
        if sharer_active != yielding {
            yielding = sharer_active;
            if let Some(channel) = control {
                send_control(&mut rtc, channel, &Control::Yielding { active: yielding });
            }
        }
        // A controlling helper's pointer takes the shape the sharer's cursor has.
        match (controlling, control) {
            (true, Some(channel)) => {
                if let Some(shape) = cursors.changed() {
                    send_control(&mut rtc, channel, &Control::Cursor(shape));
                }
            }
            _ => cursors.forget_current(),
        }
        if let Some(channel) = control {
            if let Some(trusted) = (grants.enrollment)() {
                send_control(&mut rtc, channel, &Control::Trust { id: trusted.id, secret: trusted.secret, name: trusted.name });
            }
        }
        // Text copied on the sharer's computer follows a controlling helper to theirs, whoever copied it.
        match (controlling && (grants.clipboard)(), control) {
            (true, Some(channel)) => {
                if let Some(copied) = clipboard.changed() {
                    let text = match copied {
                        crate::clipboard::Copied::Text(text) => Some(text),
                        crate::clipboard::Copied::TooLarge => None,
                    };
                    let message = Control::Clipboard { text };
                    // Characters that JSON has to spell out can make a text that fits too long to send.
                    let fits = serde_json::to_string(&message).is_ok_and(|json| json.len() <= MAX_CLIPBOARD_MESSAGE_BYTES);
                    send_control(&mut rtc, channel, if fits { &message } else { &Control::Clipboard { text: None } });
                }
            }
            _ => clipboard.forget(),
        }
        // A permission prompt, the lock screen or an administrator's window: the helper is told why
        // nothing happens.
        if let (Some(channel), true) = (control, blocked.is_none() || blocked_checked.elapsed() >= BLOCKED_CHECK_EVERY) {
            blocked_checked = Instant::now();
            let reason = crate::blocked::state();
            if blocked != Some(reason) {
                blocked = Some(reason);
                send_control(&mut rtc, channel, &Control::Blocked { reason });
            }
        }
        // The sharer closed the terminal.
        if shell.is_some() && !(grants.terminal)() {
            shell = None;
            send_terminal_closed(&mut rtc, terminal);
        }
        // A controlling helper sees their own pointer where the sharer's cursor is, so the cursor is
        // left out of the picture. It comes back whenever the pointer is the sharer's to move.
        if let Some(source) = &source {
            source.show_cursor(!controlling || sharer_active);
        }

        let step = tokio::select! {
            (local, received) = receive(&sockets, &mut buffer) => match received {
                Ok((length, source)) => Step::Input(buffer[..length].try_into().ok().map(|contents| {
                    Input::Receive(
                        Instant::now(),
                        Receive { proto: Protocol::Udp, source, destination: local, contents },
                    )
                })),
                // A reset from an unreachable peer must not end the connection.
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => Step::Input(None),
                Err(_) => return,
            },
            _ = tokio::time::sleep(wake.saturating_duration_since(Instant::now())) => Step::Input(Some(Input::Timeout(Instant::now()))),
            frame = async {
                match source.as_mut() {
                    Some(source) => source.next().await,
                    None => std::future::pending().await,
                }
            } => Step::Video(frame),
            frame = async {
                match sound.as_mut() {
                    Some((_, _, source)) => source.next().await,
                    None => std::future::pending().await,
                }
            } => Step::Sound(frame),
            event = async {
                match shell.as_mut() {
                    Some(shell) => shell.next().await,
                    None => std::future::pending().await,
                }
            } => Step::Shell(event),
        };

        match (step, video) {
            (Step::Input(Some(input)), _) => {
                if rtc.handle_input(input).is_err() {
                    return;
                }
            }
            (Step::Input(None), _) => {}
            (Step::Shell(TerminalEvent::Output(bytes)), _) => {
                if let Some(mut channel) = terminal.and_then(|channel| rtc.channel(channel)) {
                    let _ = channel.write(true, &bytes);
                }
            }
            (Step::Shell(TerminalEvent::Exited), _) => {
                shell = None;
                send_terminal_closed(&mut rtc, terminal);
            }
            (Step::Video(Some(VideoEvent::Frame(frame))), Some((mid, pt))) => send_frame(&mut rtc, mid, pt, frame, preference.content.immediate()),
            (Step::Video(Some(VideoEvent::Frame(_))), None) => {}
            (Step::Video(Some(VideoEvent::Screens { screens: list, current })), _) => {
                // The size of the screen being shown is what every other size is measured against.
                if let Some(shown) = list.iter().find(|screen| screen.id == current) {
                    let native = (shown.width.max(2) as u32, shown.height.max(2) as u32);
                    let controller = quality.get_or_insert_with(|| {
                        let mut controller = Controller::new(native);
                        controller.set_preference(preference);
                        controller
                    });
                    controller.set_native(native);
                    if let Some(source) = &source {
                        source.set_target(controller.target());
                    }
                }
                let message = Control::Screens { screens: list, current };
                if let Some(channel) = control {
                    send_control(&mut rtc, channel, &message);
                }
                screens = Some(message);
            }
            (Step::Video(Some(VideoEvent::Encoder(name))), _) => {
                let message = Control::Encoder { name };
                if let Some(channel) = control {
                    send_control(&mut rtc, channel, &message);
                }
                encoder_name = Some(message);
            }
            (Step::Video(Some(VideoEvent::Available(available))), _) => {
                screen_unavailable = !available;
                if let Some(channel) = control {
                    send_control(&mut rtc, channel, if available { &Control::ScreenAvailable } else { &Control::ScreenUnavailable });
                }
            }
            (Step::Sound(Some(frame)), _) => {
                if let Some((mid, pt, _)) = &sound {
                    let time = MediaTime::new(frame.at_sample, Frequency::FORTY_EIGHT_KHZ);
                    if let Some(writer) = rtc.writer(*mid) {
                        let _ = writer.write(*pt, Instant::now(), time, frame.data);
                    }
                }
            }
            // This computer has no sound to share; the picture carries on without it.
            (Step::Sound(None), _) => sound = None,
            // Capturing stopped for good; the data channel carries on and tells the helper why the picture is missing.
            (Step::Video(None), _) => {
                source = None;
                screen_unavailable = true;
                if let Some(channel) = control {
                    send_control(&mut rtc, channel, &Control::ScreenUnavailable);
                }
            }
        }
    }
}
