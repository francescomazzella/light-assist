//! JSON shapes shared with the UI (`packages/protocol`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    View,
    Control,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelpRequestInfo {
    /// The hosted helper page at this help request's name, while the relay can be reached. It is the
    /// address to give out when there is one; the others are for when it does not load.
    pub hosted_address: Option<String>,
    /// The same address while the relay cannot be reached: it does not work right now, and will again.
    pub hosted_offline: Option<String>,
    pub address: String,
    pub fallback_address: Option<String>,
    pub tailscale_address: Option<String>,
    pub expires_at: u64,
    pub waiting: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub helper_address: String,
    /// The helper is using the hosted helper page, and reached this computer through the relay. Their
    /// address is then the one the relay saw, which may be their whole network's.
    pub hosted: bool,
    pub started_at: u64,
    pub access: Access,
    pub control_requested: bool,
    /// The terminal grant: separate from the access level, and never implied by it.
    pub terminal: bool,
    pub terminal_requested: bool,
    /// The connection to the helper is too slow for a good picture.
    pub slow_connection: bool,
    /// Whether text copied on either computer is shared while the access level is control.
    pub clipboard: bool,
    /// The trusted device that connected by itself, with nobody here to let it in: its name.
    pub unattended: Option<String>,
    /// Whether the direct connection between the two computers is up. Until it is, the helper has
    /// been let in but sees nothing; if it never is, they are not on a network this computer is on.
    pub linked: bool,
    /// The direct connection was up and has been lost. The helper's page tries to make it again; if
    /// it stays down, the session ends by itself.
    pub link_lost: bool,
    /// Whether this helper's browser may be made a trusted device: unattended access is on, the helper
    /// is on the local network or a Tailscale one, and this session is not itself an unattended one.
    pub can_trust: bool,
}

/// Unattended access as the console shows it, present while the sharer has it switched on.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnattendedInfo {
    /// Where a trusted device connects, which stays the same from one day to the next.
    pub address: Option<String>,
    pub fallback_address: Option<String>,
    pub tailscale_address: Option<String>,
    pub hosted_address: Option<String>,
    pub hosted_offline: Option<String>,
    /// Whether this computer can be reached right now: there is a trusted device, and a port to listen on.
    pub listening: bool,
    pub devices: Vec<crate::devices::DeviceInfo>,
}

/// What the sharer's system has been told to let the app do. Only a Mac asks its user such things,
/// in System Settings, and nothing the sharer allows in the app works until they have.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    /// Seeing the screen, and hearing what the computer plays.
    pub screen: bool,
    /// Using the mouse and keyboard.
    pub control: bool,
}

/// Everything the loopback-only console renders.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalState {
    pub address: String,
    /// What kind of computer this is: `windows`, `macos` or `linux`.
    pub platform: &'static str,
    /// This app's version, such as `0.1.0`.
    pub version: &'static str,
    /// The website, if this app was built to know one: where to look for a newer version, and where
    /// the downloads and the privacy policy are.
    pub website: Option<String>,
    /// How this copy was installed, which decides who brings its updates.
    pub installation: crate::install::Installation,
    /// Absent where the system asks its user for nothing.
    pub permissions: Option<Permissions>,
    pub unattended: Option<UnattendedInfo>,
    pub request: Option<HelpRequestInfo>,
    pub session: Option<SessionInfo>,
    /// Why the last session ended by itself, until the next help request starts.
    pub ended: Option<Ended>,
    /// The keyboard shortcut that ends a session from anywhere, as it should be shown.
    pub stop_key: &'static str,
}

/// A reason for a session to end without either person ending it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ended {
    /// A helper with control or a terminal did nothing for too long.
    Idle,
    /// The session reached the longest time the sharer allows.
    TimeLimit,
    /// The direct connection between the two computers was lost and did not come back, as when
    /// either of them changes network.
    ConnectionLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Failure {
    InvalidCode,
    BadRequest,
    NotAsking,
    Busy,
    PortUnavailable,
    NotActive,
    /// The helper page and this app speak different versions of the signaling messages.
    Version,
    /// A device that is not trusted, or not on a network trusted devices may be on, tried to connect by itself.
    NotTrusted,
}

/// Signaling messages from the helper page.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum HelperMessage {
    RequestControl,
    RequestTerminal,
    Offer { sdp: String },
    /// The page binds itself to its random number and certificate, before it is told the agent's.
    Commit { protocol: u32, commit: String },
    /// The page's random number and certificate fingerprint, which must match what it committed to.
    Reveal { nonce: String, fingerprint: String },
    /// A trusted device's answer to the challenge: the keyed hash, under the device's secret, of the
    /// challenge and both ends' certificate fingerprints. With it, the page's own fingerprint.
    Proof { proof: String, fingerprint: String },
}

/// Signaling messages to the helper page.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AgentMessage {
    /// The first thing a page is told, and told again when its pairing code has grown old: which
    /// version of these messages the agent speaks, and to begin working out a pairing code.
    Hello { protocol: u32 },
    /// The agent's random number and certificate fingerprint, in answer to the page's commitment.
    Pairing { nonce: String, fingerprint: String },
    /// To a page that says it is a trusted device: prove it. With the agent's certificate fingerprint.
    Challenge { nonce: String, fingerprint: String },
    Admitted,
    Access { access: Access },
    ControlDenied,
    /// The sharer allowed the terminal, refused it, or closed it again.
    Terminal { allowed: bool },
    Answer { sdp: String },
    RtcFailed,
    Closed {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<Ended>,
    },
    Error { reason: Failure },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// Messages on the `input` data channel, honored only while the access level is control.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum InputMessage {
    /// A position on the shared screen, as fractions of its width and height.
    MouseMove { x: f64, y: f64 },
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    /// Wheel notches; positive is right and down.
    Wheel { dx: f64, dy: f64 },
    /// A physical key, named as browsers name it (`KeyboardEvent.code`, such as `KeyC` or `ArrowUp`).
    KeyDown { code: String },
    KeyUp { code: String },
    /// Typed characters, which arrive as they are whatever the two keyboard layouts.
    Text { text: String },
    /// Text the helper is about to paste: it goes onto the sharer's clipboard, and the paste follows as keys.
    Clipboard { text: String },
}

/// Messages from the helper on the `terminal` data channel, honored only with the terminal grant.
/// The shell's output travels the other way as raw bytes.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TerminalMessage {
    /// The size of the helper's terminal in character cells. The first one starts the shell.
    Resize { cols: u16, rows: u16 },
    Stdin { data: String },
}

/// Reply to the console's requests that can fail.
#[derive(Serialize)]
pub struct Outcome {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Failure>,
}

impl From<Result<(), Failure>> for Outcome {
    fn from(result: Result<(), Failure>) -> Self {
        match result {
            Ok(()) => Self { ok: true, reason: None },
            Err(reason) => Self { ok: false, reason: Some(reason) },
        }
    }
}
