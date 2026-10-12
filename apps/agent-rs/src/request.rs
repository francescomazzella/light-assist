use tokio::sync::mpsc::UnboundedSender;

use crate::{protocol::*, settings::Settings, util::now_ms};

const CODE_TTL_MS: u64 = 5 * 60_000;
const REQUEST_TTL_MS: u64 = 10 * 60_000;
/// How long a session waits for a lost connection to come back before it ends.
const LINK_GRACE_MS: u64 = 30_000;
const MAX_WAITING: usize = 8;
const MAX_WAITING_PER_ADDRESS: usize = 2;

pub enum Outgoing {
    Message(AgentMessage),
    Close,
}

/// The agent's handle on one helper page's socket.
#[derive(Clone)]
pub struct HelperChannel {
    pub id: u64,
    tx: UnboundedSender<Outgoing>,
}

impl HelperChannel {
    pub fn new(id: u64, tx: UnboundedSender<Outgoing>) -> Self {
        Self { id, tx }
    }

    pub fn send(&self, message: AgentMessage) {
        let _ = self.tx.send(Outgoing::Message(message));
    }

    fn close(&self) {
        let _ = self.tx.send(Outgoing::Close);
    }

    fn refuse(&self, reason: Failure) {
        self.send(AgentMessage::Error { reason });
        self.close();
    }
}

/// Where the helper page can be reached during a help request.
#[derive(Clone)]
pub struct Reach {
    /// The `.local` name this computer answers to, used to check the `Host` header.
    pub name: String,
    pub address: String,
    pub fallback_address: Option<String>,
    pub tailscale_address: Option<String>,
}

/// A helper page that has been shown a pairing code and is waiting for the sharer to type it.
struct Waiting {
    channel: HelperChannel,
    remote: String,
    hosted: bool,
    code: String,
    expires_at: u64,
}

struct Session {
    channel: HelperChannel,
    info: SessionInfo,
    /// When the helper last used the mouse, keyboard or terminal, or was last given one of them.
    last_activity: u64,
    /// When the direct connection was lost, having been up, if it is down now.
    link_lost_at: Option<u64>,
}

struct Open {
    reach: Reach,
    expires_at: u64,
    waiting: Vec<Waiting>,
    session: Option<Session>,
}

/// The sharer's side. While it is open, helper pages are each shown a pairing code; the one whose
/// code the sharer types becomes the session, with view access only until the sharer allows more.
#[derive(Default)]
pub struct HelpRequest {
    open: Option<Open>,
    /// Why the last session ended by itself, so the console can say so.
    ended: Option<Ended>,
}

impl HelpRequest {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn info(&self) -> Option<HelpRequestInfo> {
        self.open.as_ref().map(|open| HelpRequestInfo {
            hosted_address: None,
            hosted_offline: None,
            address: open.reach.address.clone(),
            fallback_address: open.reach.fallback_address.clone(),
            tailscale_address: open.reach.tailscale_address.clone(),
            expires_at: open.expires_at,
            waiting: open.waiting.len(),
        })
    }

    pub fn session(&self) -> Option<SessionInfo> {
        self.open.as_ref().and_then(|open| open.session.as_ref()).map(|session| session.info.clone())
    }

    pub fn has_session(&self, channel_id: u64) -> bool {
        self.open
            .as_ref()
            .and_then(|open| open.session.as_ref())
            .is_some_and(|session| session.channel.id == channel_id)
    }

    /// Whether the helper on this socket may currently use the mouse and keyboard.
    pub fn has_control(&self, channel_id: u64) -> bool {
        self.open
            .as_ref()
            .and_then(|open| open.session.as_ref())
            .is_some_and(|session| session.channel.id == channel_id && session.info.access == Access::Control)
    }

    /// Whether the helper on this socket may currently use a terminal on this computer.
    pub fn has_terminal(&self, channel_id: u64) -> bool {
        self.open
            .as_ref()
            .and_then(|open| open.session.as_ref())
            .is_some_and(|session| session.channel.id == channel_id && session.info.terminal)
    }

    fn session_of(&mut self, channel_id: u64) -> Option<&mut Session> {
        self.open.as_mut()?.session.as_mut().filter(|session| session.channel.id == channel_id)
    }

    fn current_session(&mut self) -> Option<&mut Session> {
        self.open.as_mut()?.session.as_mut()
    }

    pub fn ended(&self) -> Option<Ended> {
        self.ended
    }

    pub fn start(&mut self, reach: Reach) {
        if self.open.is_none() {
            self.open = Some(Open { reach, expires_at: now_ms() + REQUEST_TTL_MS, waiting: Vec::new(), session: None });
            self.ended = None;
        }
    }

    /// Ends the help request and any session in it.
    pub fn stop(&mut self) {
        self.stop_because(None);
    }

    fn stop_because(&mut self, reason: Option<Ended>) {
        let Some(open) = self.open.take() else { return };
        self.ended = reason;
        let channels = open.waiting.into_iter().map(|waiting| waiting.channel).chain(open.session.map(|session| session.channel));
        for channel in channels {
            channel.send(AgentMessage::Closed { reason });
            channel.close();
        }
    }

    /// The direct connection to the helper on this socket came up, or went.
    pub fn set_linked(&mut self, channel_id: u64, linked: bool) {
        if let Some(session) = self.session_of(channel_id) {
            // Only a connection that was up can be lost; one that never came up is another matter,
            // which the console describes for as long as it lasts.
            let lost = !linked && (session.info.linked || session.info.link_lost);
            session.info.linked = linked;
            session.info.link_lost = lost;
            session.link_lost_at = match (lost, session.link_lost_at) {
                (true, None) => Some(now_ms()),
                (true, since) => since,
                (false, _) => None,
            };
        }
    }

    pub fn set_slow(&mut self, channel_id: u64, slow: bool) {
        if let Some(session) = self.session_of(channel_id) {
            session.info.slow_connection = slow;
        }
    }

    /// The helper used the mouse, keyboard or terminal: the session is not idle.
    pub fn touch(&mut self, channel_id: u64) {
        if let Some(session) = self.session_of(channel_id) {
            session.last_activity = now_ms();
        }
    }

    /// Why a helper page that has just connected cannot wait here for the sharer, if it cannot.
    pub fn refusal(&self, remote: &str, hosted: bool) -> Option<Failure> {
        let Some(open) = self.open.as_ref() else { return Some(Failure::NotAsking) };
        // Pages that came through the relay all show its view of their address, which may be a whole
        // household's or office's, so they are only counted together.
        let from_same_address = open.waiting.iter().filter(|waiting| !hosted && waiting.remote == remote).count();
        (open.session.is_some() || open.waiting.len() >= MAX_WAITING || from_same_address >= MAX_WAITING_PER_ADDRESS).then_some(Failure::Busy)
    }

    /// A helper page and the agent have worked out a pairing code: the page waits, showing it, for
    /// the sharer to type it. Returns whether it may; if not, the page has been told why.
    pub fn enter(&mut self, channel: &HelperChannel, remote: &str, hosted: bool, code: String) -> bool {
        // A page working a new code out is no longer waiting with its old one.
        if let Some(open) = self.open.as_mut() {
            open.waiting.retain(|waiting| waiting.channel.id != channel.id);
        }
        if let Some(reason) = self.refusal(remote, hosted) {
            channel.refuse(reason);
            return false;
        }
        let Some(open) = self.open.as_mut() else { return false };
        // Two pages with one code, a one in a million chance: this one works out another.
        if open.waiting.iter().any(|waiting| waiting.code == code) {
            channel.send(AgentMessage::Hello { protocol: crate::pairing::PROTOCOL });
            return true;
        }

        let now = now_ms();
        open.waiting.push(Waiting { channel: channel.clone(), remote: remote.to_owned(), hosted, code, expires_at: now + CODE_TTL_MS });
        // Someone is about to read a code out, so do not close underneath them.
        open.expires_at = now + REQUEST_TTL_MS;
        true
    }

    /// The helper page's socket went away.
    pub fn leave(&mut self, channel_id: u64) {
        if self.has_session(channel_id) {
            return self.stop();
        }
        if let Some(open) = self.open.as_mut() {
            open.waiting.retain(|waiting| waiting.channel.id != channel_id);
        }
    }

    /// The sharer typed a pairing code and allowed it: that helper may now view the screen.
    pub fn admit(&mut self, code: &str) -> Result<(), Failure> {
        let open = self.open.as_mut().ok_or(Failure::NotAsking)?;
        if open.session.is_some() {
            return Err(Failure::Busy);
        }

        let now = now_ms();
        let index = open
            .waiting
            .iter()
            .position(|waiting| waiting.expires_at > now && waiting.code == code)
            .ok_or(Failure::InvalidCode)?;
        let admitted = open.waiting.swap_remove(index);

        // One helper per session: everyone else who was waiting is turned away.
        for other in open.waiting.drain(..) {
            other.channel.refuse(Failure::Busy);
        }

        admitted.channel.send(AgentMessage::Admitted);
        open.session = Some(Session {
            channel: admitted.channel,
            info: SessionInfo {
                helper_address: admitted.remote,
                hosted: admitted.hosted,
                started_at: now,
                access: Access::View,
                control_requested: false,
                terminal: false,
                terminal_requested: false,
                slow_connection: false,
                clipboard: false,
                linked: false,
                link_lost: false,
                unattended: None,
                can_trust: false,
            },
            last_activity: now,
            link_lost_at: None,
        });
        Ok(())
    }

    /// A trusted device proved itself: it becomes the session with control, with nobody here to ask.
    /// It opens a help request of its own if the sharer was not asking for help.
    pub fn admit_trusted(&mut self, channel: &HelperChannel, remote: &str, hosted: bool, device: String, reach: Reach) -> Result<(), Failure> {
        if self.session().is_some() {
            return Err(Failure::Busy);
        }
        let now = now_ms();
        self.ended = None;
        let open = self.open.get_or_insert_with(|| Open { reach, expires_at: now + REQUEST_TTL_MS, waiting: Vec::new(), session: None });
        // One helper per session: everyone who was waiting with a code is turned away.
        for other in open.waiting.drain(..) {
            other.channel.refuse(Failure::Busy);
        }

        channel.send(AgentMessage::Admitted);
        channel.send(AgentMessage::Access { access: Access::Control });
        open.session = Some(Session {
            channel: channel.clone(),
            info: SessionInfo {
                helper_address: remote.to_owned(),
                hosted,
                started_at: now,
                access: Access::Control,
                control_requested: false,
                terminal: false,
                terminal_requested: false,
                slow_connection: false,
                clipboard: false,
                linked: false,
                link_lost: false,
                unattended: Some(device),
                can_trust: false,
            },
            last_activity: now,
            link_lost_at: None,
        });
        Ok(())
    }

    /// The socket that holds the session, for something meant for that helper alone.
    pub fn session_channel(&self) -> Option<u64> {
        self.open.as_ref().and_then(|open| open.session.as_ref()).map(|session| session.channel.id)
    }

    /// The helper asks for control; the sharer must still allow it.
    pub fn request_control(&mut self, channel_id: u64) {
        if let Some(session) = self.session_of(channel_id) {
            if session.info.access == Access::View {
                session.info.control_requested = true;
            }
        }
    }

    /// The sharer's answer to a pending control request.
    pub fn answer_control(&mut self, allow: bool) {
        let Some(session) = self.current_session().filter(|session| session.info.control_requested) else { return };

        session.info.control_requested = false;
        if allow {
            // The idle clock starts when there is something to be idle with.
            session.last_activity = now_ms();
            session.info.access = Access::Control;
            session.channel.send(AgentMessage::Access { access: Access::Control });
        } else {
            session.channel.send(AgentMessage::ControlDenied);
        }
    }

    /// The sharer takes control back without ending the session.
    pub fn drop_to_view(&mut self) {
        let Some(session) = self.current_session().filter(|session| session.info.access == Access::Control) else { return };

        session.info.access = Access::View;
        session.channel.send(AgentMessage::Access { access: Access::View });
    }

    /// The helper asks for a terminal; the sharer must still allow it.
    pub fn request_terminal(&mut self, channel_id: u64) {
        if let Some(session) = self.session_of(channel_id) {
            if !session.info.terminal {
                session.info.terminal_requested = true;
            }
        }
    }

    /// The sharer's answer to a pending terminal request, or, with `allow` false, closing an open terminal.
    pub fn answer_terminal(&mut self, allow: bool) {
        let Some(session) = self.current_session() else { return };
        if !session.info.terminal_requested && (allow || !session.info.terminal) {
            return;
        }

        session.info.terminal_requested = false;
        session.info.terminal = allow;
        session.last_activity = now_ms();
        session.channel.send(AgentMessage::Terminal { allowed: allow });
    }

    pub fn expire(&mut self, settings: &Settings) {
        let Some(open) = self.open.as_mut() else { return };
        let now = now_ms();

        match &open.session {
            None if open.expires_at <= now => return self.stop(),
            Some(session) => {
                let longer_than = |since: u64, minutes: u32| minutes > 0 && now.saturating_sub(since) >= minutes as u64 * 60_000;
                // Only a helper who can act on this computer can be dangerously forgotten.
                let can_act = session.info.access == Access::Control || session.info.terminal;

                // A helper who is no longer there must not stay "connected" on the sharer's screen.
                if session.link_lost_at.is_some_and(|since| now.saturating_sub(since) >= LINK_GRACE_MS) {
                    return self.stop_because(Some(Ended::ConnectionLost));
                }
                if longer_than(session.info.started_at, settings.max_minutes) {
                    return self.stop_because(Some(Ended::TimeLimit));
                }
                if can_act && longer_than(session.last_activity, settings.idle_minutes) {
                    return self.stop_because(Some(Ended::Idle));
                }
            }
            None => {}
        }

        // A code that has grown old is not accepted any more. Its page is asked to work out another,
        // so that one left open still has a code to read out.
        open.waiting.retain(|waiting| {
            let fresh = waiting.expires_at > now;
            if !fresh {
                waiting.channel.send(AgentMessage::Hello { protocol: crate::pairing::PROTOCOL });
            }
            fresh
        });
    }
}
