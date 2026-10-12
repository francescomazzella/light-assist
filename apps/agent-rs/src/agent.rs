use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
};

use tokio::{net::TcpListener, task::JoinHandle};

use crate::{
    bonjour::Bonjour,
    devices::{DeviceInfo, Devices, Enrollment},
    lan,
    protocol::*,
    relay::{self, Hosted},
    request::{HelpRequest, HelperChannel, Reach},
    rtc,
    settings::Settings,
    stop_key::{StopKey, STOP_KEY_NAME},
    util::{is_private_network, is_tailscale, lock},
    window::Desktop,
    words,
};

struct Relay {
    task: JoinHandle<()>,
    connected: Arc<AtomicBool>,
    /// What the helper opens: the hosted helper page, at this help request's name.
    address: String,
}

pub struct Agent {
    pub console_address: String,
    /// What a request to the console's API must present; known only to the app's own window.
    pub console_key: String,
    /// Tried in order when a help request starts.
    lan_ports: Vec<u16>,
    request: Mutex<HelpRequest>,
    /// The LAN listener's task and where it can be reached. It exists while a help request is open,
    /// and, with unattended access on, for as long as there is a device to wait for.
    lan: Mutex<Option<(JoinHandle<()>, Reach)>>,
    /// The connection to the signaling relay, for helpers who use the hosted helper page. It exists
    /// exactly while the LAN listener does, if the app knows of a relay at all.
    relay: Mutex<Option<Relay>>,
    hosted: Option<Hosted>,
    /// How this copy was installed, which does not change while it runs.
    installation: crate::install::Installation,
    /// The devices trusted to connect while nobody is here.
    devices: Mutex<Devices>,
    /// What a helper's browser is about to be handed to become a trusted device, and the socket it is on.
    enrollment: Mutex<Option<(u64, Enrollment)>>,
    starting: tokio::sync::Mutex<()>,
    bonjour: Bonjour,
    next_channel: AtomicU64,
    settings: Mutex<Settings>,
    /// The app's window and on-screen indicator. Absent when the agent runs without a window.
    desktop: Mutex<Option<Desktop>>,
    /// The sharer's shortcut for ending the session, held exactly while a helper is connected.
    stop_key: Mutex<Option<StopKey>>,
    /// For work that has to call back into the agent later.
    me: Weak<Agent>,
}

/// This computer's address on the interface that carries its default route, if it is a LAN address.
fn lan_address() -> Option<Ipv4Addr> {
    // No packet is sent: connecting a UDP socket only selects a route.
    let address = rtc::local_address_towards(Ipv4Addr::new(192, 0, 2, 1)).ok()?;
    (is_private_network(address) && !is_tailscale(address)).then_some(address)
}

/// This computer's Tailscale address, if it is on a tailnet.
fn tailscale_address() -> Option<Ipv4Addr> {
    // 100.100.100.100 is Tailscale's own service address, so the route to it leaves through Tailscale.
    rtc::local_address_towards(Ipv4Addr::new(100, 100, 100, 100)).ok().filter(|address| is_tailscale(*address))
}

impl Agent {
    pub fn new(console_address: String, console_key: String, lan_ports: Vec<u16>) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            console_address,
            console_key,
            lan_ports,
            request: Mutex::new(HelpRequest::default()),
            lan: Mutex::new(None),
            relay: Mutex::new(None),
            hosted: Hosted::configured(),
            installation: crate::install::installation(),
            devices: Mutex::new(Devices::load()),
            enrollment: Mutex::new(None),
            starting: tokio::sync::Mutex::new(()),
            bonjour: Bonjour::new(),
            next_channel: AtomicU64::new(1),
            settings: Mutex::new(Settings::load()),
            desktop: Mutex::new(None),
            stop_key: Mutex::new(None),
            me: me.clone(),
        })
    }

    pub fn set_desktop(&self, desktop: Desktop) {
        *lock(&self.desktop) = Some(desktop);
    }

    /// Puts the console in front of the sharer: there is something to see or decide.
    pub fn show_window(&self) {
        if let Some(desktop) = lock(&self.desktop).as_ref() {
            (desktop.show_console)();
        }
    }

    /// Makes the on-screen indicator let the mouse through to what is beneath it, or take it again.
    /// Done by the time this returns, so that a click sent next finds the way clear.
    pub fn let_through_indicator(&self, through: bool) {
        if let Some(desktop) = lock(&self.desktop).as_ref() {
            (desktop.let_through_indicator)(through);
        }
    }

    /// Ends the app: through its window when it has one, which also says goodbye on the network.
    pub fn quit(&self) {
        match lock(&self.desktop).as_ref() {
            Some(desktop) => (desktop.quit)(),
            None => {
                self.shutdown();
                std::process::exit(0);
            }
        }
    }

    /// Whether a help request or a session is under way, which installing must not cut short.
    pub fn busy(&self) -> bool {
        lock(&self.request).is_open()
    }

    pub fn settings(&self) -> Settings {
        *lock(&self.settings)
    }

    /// Saves the settings. Unattended access changes the address this computer answers to, so it
    /// keeps its old value while a help request is open under the old address.
    pub async fn set_settings(self: &Arc<Self>, settings: Settings) {
        let before = self.settings();
        let unattended = if self.busy() { before.unattended } else { settings.unattended };
        let settings = Settings { unattended, ..settings.sane() };
        *lock(&self.settings) = settings;
        settings.save();
        self.stand_by().await;
    }

    /// Whether this computer waits for trusted devices: the sharer switched that on, and trusts one.
    fn standing(&self) -> bool {
        self.settings().unattended && !lock(&self.devices).is_empty()
    }

    /// Opens the LAN listener if there is a trusted device to wait for, and closes it if there is
    /// nothing to listen for any more. Called at start, and whenever either may have changed.
    pub async fn stand_by(self: &Arc<Self>) {
        if self.standing() {
            let _starting = self.starting.lock().await;
            if let Err(error) = self.listen().await {
                eprintln!("Trusted devices cannot connect: {error:?}");
            }
        } else {
            self.close_listener_if_idle();
        }
    }

    fn close_listener_if_idle(&self) {
        if lock(&self.request).is_open() || self.standing() {
            return;
        }
        self.close_listener();
    }

    /// Nothing answers for this computer any more: not on the LAN, and not at the relay.
    fn close_listener(&self) {
        if let Some((listener, _)) = lock(&self.lan).take() {
            listener.abort();
        }
        if let Some(relay) = lock(&self.relay).take() {
            relay.task.abort();
        }
        self.bonjour.unpublish();
    }

    /// The hosted helper page's address for this computer: first if the relay is connected, so that
    /// the address works; second if it is not, so that it does not for now.
    fn hosted_address(&self) -> (Option<String>, Option<String>) {
        match lock(&self.relay).as_ref() {
            Some(relay) if relay.connected.load(Ordering::Relaxed) => (Some(relay.address.clone()), None),
            Some(relay) => (None, Some(relay.address.clone())),
            None => (None, None),
        }
    }

    /// Whether `address` is somewhere a trusted device may be: the local network, or a Tailscale
    /// network this computer is on. Never this computer itself, and never the open internet.
    fn is_local(address: std::net::IpAddr) -> bool {
        matches!(address, std::net::IpAddr::V4(address) if address.is_private() || address.is_link_local() || is_tailscale(address))
    }

    /// Whether the helper in this session is where a trusted device may be. One who came through the
    /// relay is connected, which they could only be from a private network this computer is on.
    fn may_be_trusted(session: &SessionInfo) -> bool {
        session.hosted || session.helper_address.parse().is_ok_and(Self::is_local)
    }

    /// Opens one of the app's pages on the website in the person's own browser. Only these, never an
    /// address the console names, so that the console cannot be made to open anything else.
    pub fn open_website(&self, page: &str) -> Result<(), Failure> {
        const SOURCE: &str = "https://github.com/francescomazzella/light-assist";
        let website = self.hosted.as_ref().map(|hosted| hosted.page());
        let address = match (page, website) {
            ("source", _) => SOURCE.to_owned(),
            ("download", Some(website)) => format!("{website}/"),
            ("privacy", Some(website)) => format!("{website}/privacy"),
            _ => return Err(Failure::BadRequest),
        };
        #[cfg(windows)]
        let opened = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &address]).spawn();
        #[cfg(target_os = "macos")]
        let opened = std::process::Command::new("/usr/bin/open").arg(&address).spawn();
        #[cfg(not(any(windows, target_os = "macos")))]
        let opened = std::process::Command::new("xdg-open").arg(&address).spawn();
        opened.map(|_| ()).map_err(|_| Failure::BadRequest)
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        lock(&self.devices).list()
    }

    /// Stops trusting a device. A session it opened by itself ends with it.
    pub async fn remove_device(self: &Arc<Self>, id: &str) {
        if lock(&self.devices).remove(id) && lock(&self.request).session().is_some_and(|session| session.unattended.is_some()) {
            self.stop_request();
        }
        self.stand_by().await;
    }

    /// Makes the browser of the helper who is connected a trusted device, under the name the sharer gives it.
    pub fn trust_helper(&self, name: &str) -> Result<(), Failure> {
        let request = lock(&self.request);
        let (Some(session), Some(channel)) = (request.session(), request.session_channel()) else { return Err(Failure::NotActive) };
        if !self.settings().unattended || session.unattended.is_some() || !Self::may_be_trusted(&session) {
            return Err(Failure::BadRequest);
        }
        let enrollment = lock(&self.devices).add(name).ok_or(Failure::Busy)?;
        // Handed over on the encrypted connection, by the task that owns it.
        *lock(&self.enrollment) = Some((channel, enrollment));
        Ok(())
    }

    /// What the helper on this socket is to be handed to become a trusted device, once.
    pub fn take_enrollment(&self, channel_id: u64) -> Option<Enrollment> {
        let mut waiting = lock(&self.enrollment);
        waiting.take_if(|(channel, _)| *channel == channel_id).map(|(_, enrollment)| enrollment)
    }

    /// A page that says it is the trusted device `id` answered `challenge` with `proof`. If that holds,
    /// and it is on the local network, it becomes the session, with control.
    ///
    /// A page that came through the relay cannot be placed by its address. It is held to the same
    /// networks all the same: the connection that follows is only ever offered on private ones.
    pub fn admit_device(&self, channel: &HelperChannel, remote: &lan::Remote, id: &str, challenge: &str, proof: &str) -> Result<(), Failure> {
        if !self.settings().unattended || !remote.direct.is_none_or(Self::is_local) {
            return Err(Failure::NotTrusted);
        }
        let reach = lock(&self.lan).as_ref().map(|(_, reach)| reach.clone()).ok_or(Failure::NotAsking)?;
        let device = lock(&self.devices).verify(id, challenge, proof).ok_or(Failure::NotTrusted)?;
        self.with_request(|request| request.admit_trusted(channel, &remote.address, remote.direct.is_none(), device, reach))?;
        // Whoever is at the computer after all is shown what is happening.
        self.show_window();
        Ok(())
    }

    pub fn next_channel_id(&self) -> u64 {
        self.next_channel.fetch_add(1, Ordering::Relaxed)
    }

    /// Nothing listens on the LAN, and no name is published, once the help request has ended.
    fn with_request<R>(&self, change: impl FnOnce(&mut HelpRequest) -> R) -> R {
        let mut request = lock(&self.request);
        let (was_open, was_shared) = (request.is_open(), request.session().is_some());
        let result = change(&mut request);
        if was_open && !request.is_open() && !self.standing() {
            self.close_listener();
        }

        // The sharer is shown, on screen, exactly when someone can see it, and for exactly that long
        // has a shortcut that ends it.
        let shared = request.session().is_some();
        if shared != was_shared {
            if let Some(desktop) = lock(&self.desktop).as_ref() {
                (desktop.set_sharing)(shared);
            }
            let me = self.me.clone();
            *lock(&self.stop_key) = shared.then(|| {
                StopKey::register(move || {
                    if let Some(agent) = me.upgrade() {
                        agent.stop_request();
                    }
                })
            });
        }
        result
    }

    pub fn state(&self) -> LocalState {
        let request = lock(&self.request);
        let (hosted_address, hosted_offline) = self.hosted_address();
        LocalState {
            address: self.console_address.clone(),
            platform: std::env::consts::OS,
            version: env!("CARGO_PKG_VERSION"),
            website: self.hosted.as_ref().map(|hosted| hosted.page().to_owned()),
            installation: self.installation,
            #[cfg(target_os = "macos")]
            permissions: Some(Permissions { screen: crate::mac::may_record_screen(), control: crate::mac::may_control() }),
            #[cfg(not(target_os = "macos"))]
            permissions: None,
            request: request.info().map(|info| HelpRequestInfo { hosted_address: hosted_address.clone(), hosted_offline: hosted_offline.clone(), ..info }),
            // The settings are the sharer's, and may change during a session.
            session: request.session().map(|session| SessionInfo {
                clipboard: self.settings().clipboard,
                can_trust: self.settings().unattended && session.unattended.is_none() && Self::may_be_trusted(&session),
                ..session
            }),
            unattended: self.settings().unattended.then(|| {
                let reach = lock(&self.lan).as_ref().map(|(_, reach)| reach.clone());
                UnattendedInfo {
                    listening: reach.is_some(),
                    hosted_address: hosted_address.clone(),
                    hosted_offline: hosted_offline.clone(),
                    address: reach.as_ref().map(|reach| reach.address.clone()),
                    fallback_address: reach.as_ref().and_then(|reach| reach.fallback_address.clone()),
                    tailscale_address: reach.and_then(|reach| reach.tailscale_address),
                    devices: self.devices(),
                }
            }),
            ended: request.ended(),
            stop_key: STOP_KEY_NAME,
        }
    }

    /// Whether `hostname` is one a helper page can legitimately have been loaded from. Refusing
    /// other names blocks DNS rebinding.
    pub fn accepts_host(&self, hostname: &str) -> bool {
        match hostname.parse::<Ipv4Addr>() {
            Ok(address) => is_private_network(address) || address.is_loopback(),
            Err(_) => lock(&self.lan).as_ref().is_some_and(|(_, reach)| reach.name == hostname),
        }
    }

    pub async fn start_request(self: &Arc<Self>) -> Result<(), Failure> {
        let _starting = self.starting.lock().await;
        if lock(&self.request).is_open() {
            return Ok(());
        }
        let reach = self.listen().await?;
        // The console shows the hosted address from the start if the relay answers promptly.
        for _ in 0..60 {
            if lock(&self.relay).as_ref().is_none_or(|relay| relay.connected.load(Ordering::Relaxed)) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        lock(&self.request).start(reach);
        Ok(())
    }

    /// Opens the LAN listener, unless it is open already, and returns where it can be reached.
    async fn listen(self: &Arc<Self>) -> Result<Reach, Failure> {
        if let Some((_, reach)) = lock(&self.lan).as_ref() {
            return Ok(reach.clone());
        }

        let mut bound = None;
        for port in &self.lan_ports {
            if let Ok(listener) = TcpListener::bind((Ipv4Addr::UNSPECIFIED, *port)).await {
                bound = Some((listener, *port));
                break;
            }
        }
        let (listener, port) = bound.ok_or(Failure::PortUnavailable)?;

        // A new name for every help request, unless trusted devices are to find this computer again.
        let name = if self.settings().unattended { lock(&self.devices).address_name() } else { format!("assist-{}.local", words::pair()) };
        let port_suffix = if port == 80 { String::new() } else { format!(":{port}") };
        let by_name = format!("http://{name}{port_suffix}");
        let by_address = lan_address().map(|address| format!("http://{address}{port_suffix}"));

        // Without Bonjour the name will not resolve, so lead with the numeric address instead.
        let (address, fallback_address) = match (self.bonjour.publish(&name, port), by_address) {
            (false, Some(by_address)) => (by_address, None),
            (_, by_address) => (by_name, by_address),
        };

        let tailscale_address = tailscale_address().map(|address| format!("http://{address}{port_suffix}"));

        let reach = Reach { name, address, fallback_address, tailscale_address };
        let app = lan::router(self.clone()).into_make_service_with_connect_info::<SocketAddr>();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        *lock(&self.lan) = Some((task, reach.clone()));

        // The same name is the room at the relay, for helpers who use the hosted helper page.
        if let Some(hosted) = &self.hosted {
            let room = reach.name.trim_start_matches("assist-").trim_end_matches(".local").to_owned();
            let connected = Arc::new(AtomicBool::new(false));
            let address = hosted.address(&room);
            let task = tokio::spawn(relay::run(self.clone(), hosted.clone(), room, connected.clone()));
            *lock(&self.relay) = Some(Relay { task, connected, address });
        }
        Ok(reach)
    }

    pub fn stop_request(&self) {
        self.with_request(|request| request.stop());
    }

    pub fn expire(&self) {
        let settings = self.settings();
        self.with_request(|request| request.expire(&settings));
    }

    pub fn admit(&self, code: &str) -> Result<(), Failure> {
        self.with_request(|request| request.admit(code))
    }

    pub fn answer_control(&self, allow: bool) {
        self.with_request(|request| request.answer_control(allow));
    }

    pub fn drop_to_view(&self) {
        self.with_request(|request| request.drop_to_view());
    }

    /// Why a helper page that has just connected cannot wait for the sharer, if it cannot.
    pub fn refusal(&self, remote: &lan::Remote) -> Option<Failure> {
        lock(&self.request).refusal(&remote.address, remote.direct.is_none())
    }

    /// A helper page has a pairing code to show, and waits for the sharer to type it.
    pub fn enter(&self, channel: &HelperChannel, remote: &lan::Remote, code: String) -> bool {
        self.with_request(|request| request.enter(channel, &remote.address, remote.direct.is_none(), code))
    }

    /// The direct connection to the helper on this socket came up, or went.
    pub fn set_linked(&self, channel_id: u64, linked: bool) {
        lock(&self.request).set_linked(channel_id, linked);
    }

    pub fn leave(&self, channel_id: u64) {
        self.with_request(|request| request.leave(channel_id));
    }

    pub fn request_control(&self, channel_id: u64) {
        self.with_request(|request| request.request_control(channel_id));
        // The question is in the console, which may be closed to the tray.
        self.show_window();
    }

    pub fn has_session(&self, channel_id: u64) -> bool {
        lock(&self.request).has_session(channel_id)
    }

    pub fn has_control(&self, channel_id: u64) -> bool {
        lock(&self.request).has_control(channel_id)
    }

    pub fn request_terminal(&self, channel_id: u64) {
        self.with_request(|request| request.request_terminal(channel_id));
        self.show_window();
    }

    pub fn answer_terminal(&self, allow: bool) {
        self.with_request(|request| request.answer_terminal(allow));
    }

    /// Whether text copied on either computer currently follows to the other.
    pub fn shares_clipboard(&self, channel_id: u64) -> bool {
        self.settings().clipboard && self.has_control(channel_id)
    }

    pub fn has_terminal(&self, channel_id: u64) -> bool {
        lock(&self.request).has_terminal(channel_id)
    }

    /// The connection to the helper on this socket became slow, or recovered.
    pub fn set_slow(&self, channel_id: u64, slow: bool) {
        lock(&self.request).set_slow(channel_id, slow);
    }

    /// The helper on this socket just used the mouse, keyboard or terminal.
    pub fn touch(&self, channel_id: u64) {
        lock(&self.request).touch(channel_id);
    }

    pub fn shutdown(&self) {
        self.stop_request();
        self.close_listener();
        self.bonjour.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_local_network_counts_as_local() {
        let local = |address: &str| Agent::is_local(address.parse().unwrap());
        assert!(local("192.168.1.20") && local("10.0.0.7") && local("172.16.4.1") && local("169.254.10.10"));
        // A Tailscale network counts: only its own members can reach an address on it.
        assert!(local("100.101.102.103"));
        // This computer itself, the internet (100.128 and up is no longer Tailscale's), and anything that is not IPv4.
        assert!(!local("127.0.0.1") && !local("8.8.8.8") && !local("100.128.0.1") && !local("fe80::1"));
    }
}
