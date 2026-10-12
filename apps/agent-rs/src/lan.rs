//! The LAN listener: serves the helper page and its signaling socket, only during a help request.
//!
//! Signaling itself, the conversation with one helper page, is the same whether the page's socket
//! arrives here or through the relay (`relay.rs`), and is written once, for either.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Request, State,
    },
    http::{header, HeaderValue, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tokio::sync::mpsc;

use crate::{
    agent::Agent,
    assets, pairing,
    protocol::*,
    request::{HelperChannel, Outgoing},
    rtc,
};

const MAX_MESSAGE_BYTES: usize = 32 * 1024;

pub fn router(agent: Arc<Agent>) -> Router {
    Router::new()
        .route("/ws", get(upgrade))
        .fallback(get(page))
        .layer(middleware::from_fn_with_state(agent.clone(), guard))
        .with_state(agent)
}

/// Only pages loaded from this listener, under one of its own names, may talk to it.
async fn guard(State(agent): State<Arc<Agent>>, request: Request, next: Next) -> Response {
    let allowed = assets::same_site_hostname(request.headers()).is_some_and(|hostname| agent.accepts_host(&hostname));
    if !allowed {
        return StatusCode::FORBIDDEN.into_response();
    }

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("frame-ancestors 'none'"));
    response
}

async fn page(uri: Uri) -> Response {
    assets::serve(uri.path(), "helper.html")
}

/// One helper page's signaling socket, however it reaches the agent.
pub trait Wire {
    /// The next message from the page; `None` once it has gone.
    async fn recv(&mut self) -> Option<String>;
    /// Sends a message to the page, and reports whether it could be.
    async fn send(&mut self, message: String) -> bool;
    async fn close(&mut self);
}

/// Where a helper page is.
pub struct Remote {
    /// For the sharer to read.
    pub address: String,
    /// The page's own address, when it connected to this computer directly. Through the relay only
    /// the relay's view of it is known, which says nothing about the network the helper is on.
    pub direct: Option<IpAddr>,
}

struct LanSocket(WebSocket);

impl Wire for LanSocket {
    async fn recv(&mut self) -> Option<String> {
        loop {
            match self.0.recv().await {
                Some(Ok(Message::Text(raw))) => return Some(raw.to_string()),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return None,
                Some(Ok(_)) => {}
            }
        }
    }

    async fn send(&mut self, message: String) -> bool {
        self.0.send(Message::Text(message.into())).await.is_ok()
    }

    async fn close(&mut self) {
        let _ = self.0.send(Message::Close(None)).await;
    }
}

async fn upgrade(
    State(agent): State<Arc<Agent>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    uri: Uri,
    socket: WebSocketUpgrade,
) -> Response {
    // A page that holds a trusted device's secret says which device it is as it connects.
    let device = uri.query().and_then(|query| query.split('&').find_map(|pair| pair.strip_prefix("device="))).filter(|id| id.len() <= 64).map(str::to_owned);
    let address = remote.ip().to_canonical();
    socket
        .max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| serve(agent, LanSocket(socket), Remote { address: address.to_string(), direct: Some(address) }, device))
}

fn text(message: &AgentMessage) -> String {
    serde_json::to_string(message).unwrap_or_default()
}

/// How long a page has to answer the challenge.
const PROOF_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

/// One helper page's conversation, as far as it has got.
struct Conversation {
    channel: HelperChannel,
    remote: Remote,
    /// The certificate this helper's connection is encrypted with, from the agent's side.
    identity: rtc::Identity,
    /// The agent's random number for the pairing code being worked out, once the page has committed
    /// to its own, and that commitment.
    pairing: Option<(String, String)>,
    /// The fingerprint of the certificate the page paired with, which its connection must come from.
    helper_fingerprint: Option<String>,
    /// Dropping the link closes the WebRTC connection, so it lives exactly as long as this socket.
    link: Option<rtc::RtcLink>,
}

/// Asks a page that says it is a trusted device to prove it, and lets it in if it does.
async fn admit_device(agent: &Arc<Agent>, wire: &mut impl Wire, talk: &mut Conversation, id: &str) -> Result<(), Failure> {
    let nonce = crate::devices::random_hex(16);
    if !wire.send(text(&AgentMessage::Challenge { nonce: nonce.clone(), fingerprint: talk.identity.fingerprint.clone() })).await {
        return Err(Failure::NotTrusted);
    }
    let Ok(Some(raw)) = tokio::time::timeout(PROOF_WITHIN, wire.recv()).await else { return Err(Failure::NotTrusted) };
    let Ok(HelperMessage::Proof { proof, fingerprint }) = serde_json::from_str(&raw) else { return Err(Failure::NotTrusted) };
    if !pairing::is_fingerprint(&fingerprint) {
        return Err(Failure::NotTrusted);
    }
    // The proof covers both certificates, so it lets in this connection and no other.
    let challenge = pairing::device_challenge(&nonce, &talk.identity.fingerprint, &fingerprint);
    agent.admit_device(&talk.channel, &talk.remote, id, &challenge, &proof)?;
    talk.helper_fingerprint = Some(fingerprint);
    Ok(())
}

/// Only a helper the sharer has admitted may open a media/data connection to this computer.
async fn answer(agent: &Arc<Agent>, talk: &mut Conversation, sdp: &str) -> AgentMessage {
    let channel_id = talk.channel.id;
    let (true, Some(expected)) = (agent.has_session(channel_id), talk.helper_fingerprint.as_deref()) else {
        return AgentMessage::Error { reason: Failure::NotActive };
    };
    // Towards a page that connected here, the way to it is known. Through the relay it is not, and
    // the connection is offered on every private network this computer is on.
    let towards = match talk.remote.direct {
        Some(IpAddr::V4(helper)) => Some(helper),
        Some(IpAddr::V6(_)) => return AgentMessage::RtcFailed,
        None => None,
    };

    // Asked again for every message, so taking a permission back takes effect at once.
    let (controller, copier, typist, witness, reporter, truster, linker, bar) =
        (agent.clone(), agent.clone(), agent.clone(), agent.clone(), agent.clone(), agent.clone(), agent.clone(), agent.clone());
    let grants = rtc::Grants {
        control: Box::new(move || controller.has_control(channel_id)),
        clipboard: Box::new(move || copier.shares_clipboard(channel_id)),
        enrollment: Box::new(move || truster.take_enrollment(channel_id)),
        terminal: Box::new(move || typist.has_terminal(channel_id)),
        activity: Box::new(move || witness.touch(channel_id)),
        let_through: Arc::new(move |through| bar.let_through_indicator(through)),
        linked: Box::new(move |linked| linker.set_linked(channel_id, linked)),
        slow: Box::new(move |slow| reporter.set_slow(channel_id, slow)),
    };

    match rtc::accept(sdp, towards, &talk.identity, expected, grants).await {
        Ok((answer, new_link)) => {
            talk.link.replace(new_link);
            AgentMessage::Answer { sdp: answer }
        }
        Err(error) => {
            eprintln!("SDP offer rejected: {error}");
            AgentMessage::RtcFailed
        }
    }
}

/// Signaling with one helper page, from its arrival until it or the session goes.
pub async fn serve(agent: Arc<Agent>, mut wire: impl Wire, remote: Remote, device: Option<String>) {
    let (tx, mut outgoing) = mpsc::unbounded_channel();
    let channel = HelperChannel::new(agent.next_channel_id(), tx);
    let Ok(identity) = rtc::Identity::new() else {
        wire.send(text(&AgentMessage::RtcFailed)).await;
        return wire.close().await;
    };
    let mut talk = Conversation { channel: channel.clone(), remote, identity, pairing: None, helper_fingerprint: None, link: None };

    let refused = match device {
        Some(id) => admit_device(&agent, &mut wire, &mut talk, &id).await.err(),
        // A page nobody is waiting for is told at once; one that is welcome starts on a pairing code.
        None => agent.refusal(&talk.remote).or_else(|| {
            channel.send(AgentMessage::Hello { protocol: pairing::PROTOCOL });
            None
        }),
    };
    if let Some(reason) = refused {
        wire.send(text(&AgentMessage::Error { reason })).await;
        return wire.close().await;
    }

    loop {
        tokio::select! {
            out = outgoing.recv() => match out {
                Some(Outgoing::Message(message)) => {
                    if !wire.send(text(&message)).await {
                        break;
                    }
                }
                Some(Outgoing::Close) | None => break,
            },
            incoming = wire.recv() => match incoming.as_deref().map(serde_json::from_str::<HelperMessage>) {
                Some(Ok(HelperMessage::Commit { protocol, .. })) if protocol != pairing::PROTOCOL => {
                    wire.send(text(&AgentMessage::Error { reason: Failure::Version })).await;
                    break;
                }
                // The page has bound itself to its number and certificate: now it may know the agent's.
                Some(Ok(HelperMessage::Commit { commit, .. })) if !agent.has_session(channel.id) => {
                    let nonce = crate::devices::random_hex(16);
                    talk.pairing = Some((nonce.clone(), commit));
                    channel.send(AgentMessage::Pairing { nonce, fingerprint: talk.identity.fingerprint.clone() });
                }
                Some(Ok(HelperMessage::Reveal { nonce, fingerprint })) => {
                    let kept = talk.pairing.take().filter(|(_, commit)| pairing::is_fingerprint(&fingerprint) && *commit == pairing::commitment(&nonce, &fingerprint));
                    let Some((own_nonce, _)) = kept else {
                        channel.send(AgentMessage::Error { reason: Failure::BadRequest });
                        continue;
                    };
                    let code = pairing::code(&fingerprint, &talk.identity.fingerprint, &nonce, &own_nonce);
                    talk.helper_fingerprint = Some(fingerprint);
                    agent.enter(&channel, &talk.remote, code);
                }
                Some(Ok(HelperMessage::RequestControl)) => agent.request_control(channel.id),
                Some(Ok(HelperMessage::RequestTerminal)) => agent.request_terminal(channel.id),
                // Only asked for, and only answered, before a device is let in; and no new pairing
                // code is worked out once a session is under way.
                Some(Ok(HelperMessage::Proof { .. } | HelperMessage::Commit { .. })) => {}
                Some(Ok(HelperMessage::Offer { sdp })) => {
                    let reply = answer(&agent, &mut talk, &sdp).await;
                    if !wire.send(text(&reply)).await {
                        break;
                    }
                }
                Some(Err(_)) => channel.send(AgentMessage::Error { reason: Failure::BadRequest }),
                None => break,
            },
        }
    }

    drop(talk.link.take());
    agent.leave(channel.id);
    wire.close().await;
}
