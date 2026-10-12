//! The agent's connection to the signaling relay, for helpers who use the hosted helper page.
//!
//! A page loaded over HTTPS cannot open a plain socket to a computer on the LAN, so the two meet at a
//! relay instead (`apps/relay`): the agent connects out to a room named by its help address, and each
//! helper page that opens that address becomes a conversation in it. What is said is exactly what is
//! said on the LAN listener's sockets, and `lan::serve` holds it. Only signaling passes this way; the
//! session itself is still directly between the two computers.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::{
    agent::Agent,
    lan::{self, Remote, Wire},
};

/// How often the relay is told the agent is still there, which also keeps the connection from being
/// dropped by whatever sits in between.
const PING_EVERY: Duration = Duration::from_secs(15);
/// With nothing heard for this long, not even an answer to a ping, the connection is taken for dead.
const SILENCE: Duration = Duration::from_secs(40);
const RETRY_AT_MOST_EVERY: Duration = Duration::from_secs(30);

/// Where the relay and the hosted helper page are. Without both, helpers use the LAN page only.
#[derive(Clone)]
pub struct Hosted {
    /// The relay's address, such as `wss://light-assist-relay.example.workers.dev`.
    relay: String,
    /// The hosted helper page's, such as `https://assist.example.com`.
    page: String,
}

impl Hosted {
    /// Set when the app is built (`LIGHT_ASSIST_RELAY_URL`, `LIGHT_ASSIST_PAGE_URL`), and by the same
    /// names in the environment when it runs, for development and tests.
    pub fn configured() -> Option<Self> {
        let setting = |name: &str, built_in: Option<&str>| {
            std::env::var(name).ok().or(built_in.map(str::to_owned)).map(|value| value.trim_end_matches('/').to_owned()).filter(|value| !value.is_empty())
        };
        Some(Self {
            relay: setting("LIGHT_ASSIST_RELAY_URL", option_env!("LIGHT_ASSIST_RELAY_URL"))?,
            page: setting("LIGHT_ASSIST_PAGE_URL", option_env!("LIGHT_ASSIST_PAGE_URL"))?,
        })
    }

    /// The website itself, where the app's downloads and the privacy policy are.
    pub fn page(&self) -> &str {
        &self.page
    }

    /// What the helper opens for the help request named `room`.
    pub fn address(&self, room: &str) -> String {
        format!("{}/{room}", self.page)
    }
}

/// What the relay says to the agent: which helper page, and what about it.
#[derive(Deserialize)]
struct FromRelay {
    from: u64,
    #[serde(default)]
    joined: bool,
    #[serde(default)]
    left: bool,
    #[serde(default)]
    address: String,
    device: Option<String>,
    message: Option<serde_json::Value>,
}

/// One helper page's conversation, carried inside the agent's single connection to the relay.
struct RelayWire {
    id: u64,
    incoming: mpsc::UnboundedReceiver<String>,
    outgoing: mpsc::UnboundedSender<String>,
}

impl Wire for RelayWire {
    async fn recv(&mut self) -> Option<String> {
        self.incoming.recv().await
    }

    async fn send(&mut self, message: String) -> bool {
        // The message is JSON already, and goes inside the envelope as it is.
        self.outgoing.send(format!(r#"{{"to":{},"message":{message}}}"#, self.id)).is_ok()
    }

    async fn close(&mut self) {
        let _ = self.outgoing.send(format!(r#"{{"to":{},"close":true}}"#, self.id));
    }
}

/// Stays connected to the relay as `room` until dropped, and says whether it is through `connected`.
pub async fn run(agent: Arc<Agent>, hosted: Hosted, room: String, connected: Arc<AtomicBool>) {
    let url = format!("{}/agent/{room}", hosted.relay);
    let mut wait = Duration::from_secs(1);
    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok((socket, _)) => {
                connected.store(true, Ordering::Relaxed);
                wait = Duration::from_secs(1);
                converse(&agent, socket).await;
                connected.store(false, Ordering::Relaxed);
            }
            Err(error) => eprintln!("The hosted helper page cannot be used for now: {error}"),
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(RETRY_AT_MOST_EVERY);
    }
}

async fn converse(agent: &Arc<Agent>, socket: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>) {
    let (mut to_relay, mut from_relay) = socket.split();
    let (outgoing, mut queued) = mpsc::unbounded_channel::<String>();
    // Each helper page's incoming messages. Dropping one ends that page's conversation, and dropping
    // them all, when this returns, ends every one of them.
    let mut helpers: HashMap<u64, mpsc::UnboundedSender<String>> = HashMap::new();
    let mut ping = tokio::time::interval(PING_EVERY);

    loop {
        tokio::select! {
            Some(message) = queued.recv() => {
                if to_relay.send(Message::Text(message.into())).await.is_err() {
                    return;
                }
            }
            _ = ping.tick() => {
                if to_relay.send(Message::Text("ping".into())).await.is_err() {
                    return;
                }
            }
            heard = tokio::time::timeout(SILENCE, from_relay.next()) => {
                let Ok(Some(Ok(message))) = heard else { return };
                let Message::Text(raw) = message else { continue };
                let Ok(said) = serde_json::from_str::<FromRelay>(&raw) else { continue };

                if said.joined {
                    let (to_helper, incoming) = mpsc::unbounded_channel();
                    helpers.insert(said.from, to_helper);
                    let wire = RelayWire { id: said.from, incoming, outgoing: outgoing.clone() };
                    let remote = Remote { address: said.address, direct: None };
                    tokio::spawn(lan::serve(agent.clone(), wire, remote, said.device.filter(|id| id.len() <= 64)));
                } else if said.left {
                    helpers.remove(&said.from);
                } else if let (Some(message), Some(helper)) = (said.message, helpers.get(&said.from)) {
                    let _ = helper.send(message.to_string());
                }
            }
        }
    }
}
