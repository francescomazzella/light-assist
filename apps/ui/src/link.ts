import { ref, shallowRef, watch } from "vue";
import { forgetClipboard, receiveClipboard } from "./clipboard.js";
import { rememberDevice } from "./device.js";
import { hosted } from "./hosted.js";
import { t } from "./i18n.js";
import { agentFingerprint, fingerprintIn, ownIdentity } from "./pairing.js";
import { connectionSlow, qualityChoice, qualityTarget, type QualityTarget } from "./quality.js";
import type { InputMessage, TerminalMessage } from "@light-assist/protocol";

export type LinkPhase = "idle" | "connecting" | "open" | "failed";

export const linkPhase = ref<LinkPhase>("idle");
export const linkError = ref("");
export const linkLatency = ref<number | null>(null);
/** The sharer's agent reported that it cannot capture the screen, for instance because it is locked. */
export const screenUnavailable = ref(false);

/** What kind of computer the sharer's is, which decides what its keys and shortcuts are. */
export const sharerPlatform = ref<"windows" | "macos">("windows");

/** One of the sharer's monitors. */
export interface ScreenInfo {
  id: string;
  width: number;
  height: number;
  primary: boolean;
}
/** The sharer's monitors, primary first, and the one being shown. */
export const screens = ref<ScreenInfo[]>([]);
export const currentScreen = ref("");
/** The encoder making the picture on the sharer's computer, as their system names it. */
export const encoderName = ref("");
/** The sharer is using their own mouse or keyboard, so the helper's input is being set aside for a moment. */
export const yielding = ref(false);
/** The shape of the sharer's cursor, as a CSS `cursor` value. */
export const remoteCursor = ref("default");
const BLOCKS = ["protected-screen", "elevated-window", "screen-not-permitted", "control-not-permitted"] as const;
/** Why the sharer's system is keeping the helper out of the sharer's screen, if it is. */
export const blocked = ref<(typeof BLOCKS)[number] | null>(null);
/** Cursors a program drew for itself, by the name the sharer's agent gave each. Sent once, kept here. */
const drawnCursors = new Map<string, string>();
/** The sharer's screen, once the browser has received the video track. */
export const remoteVideo = shallowRef<MediaStream | null>(null);
/** What the sharer's computer plays, when it shares its sound. */
export const remoteSound = shallowRef<MediaStream | null>(null);

export interface VideoStats {
  width: number;
  height: number;
  fps: number;
  /** Average time a frame waited in the browser's jitter buffer over the last second. */
  bufferMs: number;
  /** Average time the browser took to decode a frame over the last second. */
  decodeMs: number;
}
export const videoStats = ref<VideoStats | null>(null);

/** Carries an SDP offer to the sharer's agent and resolves with its SDP answer. */
export type Signal = (sdp: string) => Promise<string>;

/** Where the terminal panel receives the shell's output and learns that the shell is gone. */
export const terminalSink: { output?: (bytes: Uint8Array) => void; closed?: () => void } = {};

let connection: RTCPeerConnection | undefined;
let inputChannel: RTCDataChannel | undefined;
let terminalChannel: RTCDataChannel | undefined;
let controlChannel: RTCDataChannel | undefined;
let pinger: number | undefined;
let statsTimer: number | undefined;
let viewerTimer: number | undefined;
/** For making the connection again by itself when it is lost, as when either computer changes network. */
let lastSignal: Signal | undefined;
let retryTimer: number | undefined;
let retries = 0;
const RETRIES = 6;
const RETRY_AFTER_MS = 3000;

export function closeLink() {
  window.clearTimeout(retryTimer);
  window.clearInterval(pinger);
  window.clearInterval(statsTimer);
  window.clearInterval(viewerTimer);
  qualityTarget.value = null;
  connectionSlow.value = false;
  remoteVideo.value = null;
  remoteSound.value = null;
  videoStats.value = null;
  screenUnavailable.value = false;
  screens.value = [];
  currentScreen.value = "";
  encoderName.value = "";
  controlChannel = undefined;
  yielding.value = false;
  remoteCursor.value = "default";
  blocked.value = null;
  drawnCursors.clear();
  forgetClipboard();
  const old = connection;
  connection = undefined;
  inputChannel = undefined;
  terminalChannel = undefined;
  old?.close();
  linkPhase.value = "idle";
  linkLatency.value = null;
}

function iceGathered(peer: RTCPeerConnection, timeoutMs: number) {
  return new Promise<void>((resolve) => {
    if (peer.iceGatheringState === "complete") return resolve();
    const timer = window.setTimeout(resolve, timeoutMs);
    peer.addEventListener("icegatheringstatechange", () => {
      if (peer.iceGatheringState !== "complete") return;
      window.clearTimeout(timer);
      resolve();
    });
  });
}

/** Reports what the browser is actually doing with the video, so latency is measured, not assumed. */
function watchVideo(peer: RTCPeerConnection) {
  let last = { emitted: 0, buffered: 0, decoded: 0, decoding: 0 };

  statsTimer = window.setInterval(async () => {
    const report = await peer.getStats().catch(() => undefined);
    if (connection !== peer || !report) return;

    report.forEach((entry: Record<string, unknown>) => {
      if (entry.type !== "inbound-rtp" || entry.kind !== "video") return;
      const number = (key: string) => (typeof entry[key] === "number" ? (entry[key] as number) : 0);
      const now = {
        emitted: number("jitterBufferEmittedCount"),
        buffered: number("jitterBufferDelay"),
        decoded: number("framesDecoded"),
        decoding: number("totalDecodeTime"),
      };
      const frames = { emitted: now.emitted - last.emitted, decoded: now.decoded - last.decoded };
      videoStats.value = {
        width: number("frameWidth"),
        height: number("frameHeight"),
        fps: Math.round(number("framesPerSecond")),
        bufferMs: frames.emitted > 0 ? Math.round(((now.buffered - last.buffered) / frames.emitted) * 1000) : 0,
        decodeMs: frames.decoded > 0 ? Math.round(((now.decoding - last.decoding) / frames.decoded) * 1000) : 0,
      };
      last = now;
    });
  }, 1000);
}

/** Sends mouse input to the sharer's computer. The sharer's agent ignores it unless control is allowed. */
export function sendInput(message: InputMessage) {
  if (inputChannel?.readyState === "open") inputChannel.send(JSON.stringify(message));
}

/** Sends typing or a new size to the shell. The sharer's agent ignores it without the terminal grant. */
/**
 * Gives the helper's pointer the shape of the sharer's cursor. Everything that ends up in a style is
 * checked first: a keyword, a PNG in a `data:` address, and two small numbers.
 */
function showCursor(message: Record<string, unknown>) {
  const keyword = /^[a-z-]{1,20}$/.test(String(message.shape)) ? String(message.shape) : "default";
  const id = typeof message.id === "string" && /^[0-9a-f]{16}$/.test(message.id) ? message.id : null;
  if (!id) {
    remoteCursor.value = keyword;
    return;
  }
  if (typeof message.image === "string" && /^data:image\/png;base64,[A-Za-z0-9+/=]{1,200000}$/.test(message.image)) {
    const hotspot: unknown[] = Array.isArray(message.hotspot) ? message.hotspot : [];
    const spot = [0, 1].map((index) => {
      const value = Number(hotspot[index]);
      return Number.isInteger(value) && value >= 0 && value < 128 ? value : 0;
    });
    drawnCursors.set(id, `url(${message.image}) ${spot[0]} ${spot[1]}, ${keyword}`);
  }
  remoteCursor.value = drawnCursors.get(id) ?? keyword;
}

/** Tells the sharer's agent the helper's quality choices; anything left automatic it decides itself. */
function sendQualityChoice() {
  if (controlChannel?.readyState === "open") controlChannel.send(JSON.stringify({ type: "quality-preference", ...qualityChoice.value }));
}
watch(qualityChoice, sendQualityChoice, { deep: true });

let droppedBefore = 0;
let shownBefore = 0;
/**
 * Tells the sharer's agent how large the picture is here, in the screen's own pixels, so that it sends
 * no more than can be seen, and whether this browser is dropping pictures it cannot decode in time.
 */
function reportViewer() {
  const video = document.querySelector<HTMLVideoElement>("video.viewer-screen");
  if (!video || controlChannel?.readyState !== "open") return;
  const scale = window.devicePixelRatio || 1;
  const { droppedVideoFrames: dropped, totalVideoFrames: shown } = video.getVideoPlaybackQuality();
  // A few dropped pictures are normal on any connection; a fifth of them is a browser that cannot keep up.
  const struggling = shown - shownBefore >= 10 && dropped - droppedBefore > (shown - shownBefore) / 5;
  [droppedBefore, shownBefore] = [dropped, shown];
  controlChannel.send(
    JSON.stringify({ type: "viewer", width: Math.round(video.clientWidth * scale), height: Math.round(video.clientHeight * scale), struggling }),
  );
}

/** Asks the sharer's agent to show another of their monitors. */
export function selectScreen(id: string) {
  if (controlChannel?.readyState === "open") controlChannel.send(JSON.stringify({ type: "select-screen", id }));
}

export function sendTerminal(message: TerminalMessage) {
  if (terminalChannel?.readyState === "open") terminalChannel.send(JSON.stringify(message));
}

/**
 * The two computers found no way to each other. They connect directly, so they have to share a
 * network; the hosted page can be opened from anywhere, which makes this the likely reason there.
 */
const unreachable = () =>
  hosted
    ? t("This computer could not reach theirs. Light Assist connects two computers directly: both have to be on the same network (the same Wi-Fi, for instance) or on the same Tailscale network.")
    : t("This computer could not reach theirs directly. A firewall on either one may be in the way.");

function fail(peer: RTCPeerConnection, message: string) {
  if (connection !== peer) return;
  closeLink();
  linkPhase.value = "failed";
  linkError.value = message;
}

/** Opens the direct connection to the sharer's computer once the sharer has admitted this helper. */
export async function openLink(signal: Signal) {
  closeLink();
  lastSignal = signal;
  linkPhase.value = "connecting";
  linkError.value = "";

  // The two computers share a LAN, so no STUN or TURN server is involved. The certificate is the
  // one the pairing code was worked out from.
  let certificate: RTCCertificate;
  try {
    ({ certificate } = await ownIdentity());
  } catch (error) {
    linkPhase.value = "failed";
    linkError.value = error instanceof Error ? error.message : t("The connection could not be set up.");
    return;
  }
  const peer = new RTCPeerConnection({ iceServers: [], certificates: [certificate] });
  connection = peer;
  let everConnected = false;

  const video = peer.addTransceiver("video", { direction: "recvonly" });
  // Show frames as they arrive: on a LAN the default smoothing delay buys nothing.
  (video.receiver as RTCRtpReceiver & { jitterBufferTarget?: number | null }).jitterBufferTarget = 0;
  // The sound of the sharer's computer comes with its screen.
  peer.addTransceiver("audio", { direction: "recvonly" });
  peer.ontrack = (event) => {
    if (connection !== peer) return;
    // Each in a stream of its own: the picture must never wait for the sound.
    if (event.track.kind === "audio") remoteSound.value = new MediaStream([event.track]);
    else remoteVideo.value = new MediaStream([event.track]);
  };
  watchVideo(peer);

  inputChannel = peer.createDataChannel("input");

  // The shell's output arrives as raw bytes; the only text message says that the shell is gone.
  const shell = peer.createDataChannel("terminal");
  shell.binaryType = "arraybuffer";
  shell.onmessage = (event) => {
    if (connection !== peer) return;
    if (event.data instanceof ArrayBuffer) terminalSink.output?.(new Uint8Array(event.data));
    else terminalSink.closed?.();
  };
  terminalChannel = shell;

  const channel = peer.createDataChannel("control");
  controlChannel = channel;
  channel.onopen = () => {
    if (connection !== peer) return;
    linkPhase.value = "open";
    pinger = window.setInterval(() => {
      if (channel.readyState === "open") channel.send(JSON.stringify({ type: "ping", timestamp: performance.now() }));
    }, 2000);
    sendQualityChoice();
    viewerTimer = window.setInterval(reportViewer, 2000);
  };
  channel.onmessage = (event) => {
    try {
      const message = JSON.parse(String(event.data)) as { type?: string; timestamp?: number; active?: boolean; reason?: unknown; platform?: unknown; text?: unknown };
      if (message.type === "hello") sharerPlatform.value = message.platform === "macos" ? "macos" : "windows";
      if (message.type === "trust") rememberDevice(message as Record<string, unknown>);
      if (message.type === "clipboard") void receiveClipboard(typeof message.text === "string" ? message.text : null);
      if (message.type === "screen-unavailable") screenUnavailable.value = true;
      if (message.type === "screen-available") screenUnavailable.value = false;
      if (message.type === "encoder") encoderName.value = (message as unknown as { name: string }).name;
      if (message.type === "quality") {
        const update = message as unknown as { target: QualityTarget; slow: boolean };
        qualityTarget.value = update.target;
        connectionSlow.value = update.slow;
      }
      if (message.type === "screens") {
        const update = message as unknown as { screens: ScreenInfo[]; current: string };
        screens.value = update.screens;
        currentScreen.value = update.current;
      }
      if (message.type === "yielding") yielding.value = message.active === true;
      if (message.type === "cursor") showCursor(message);
      if (message.type === "blocked") blocked.value = BLOCKS.find((reason) => reason === message.reason) ?? null;
      if (message.type === "pong" && typeof message.timestamp === "number") {
        linkLatency.value = Math.round(performance.now() - message.timestamp);
      }
    } catch {
      // Ignore anything that is not a control message.
    }
  };
  peer.onconnectionstatechange = () => {
    if (peer.connectionState === "connected") {
      everConnected = true;
      retries = 0;
    }
    if (peer.connectionState === "failed" || peer.connectionState === "disconnected") {
      if (connection !== peer) return;
      // A connection that was up is made again, a few times, before the helper is left to decide:
      // the sharer's computer waits half a minute for exactly this.
      const again = everConnected && retries < RETRIES;
      fail(peer, !everConnected ? unreachable() : again ? t("The connection to the other computer was lost. Trying again…") : t("The connection to the other computer was lost."));
      if (again) {
        retries++;
        retryTimer = window.setTimeout(() => lastSignal && linkPhase.value === "failed" && void openLink(lastSignal), RETRY_AFTER_MS);
      }
    }
  };

  try {
    await peer.setLocalDescription(await peer.createOffer());
    await iceGathered(peer, 3000);

    const answer = await signal(peer.localDescription?.sdp ?? "");
    if (connection !== peer) return;
    // Whoever answers must be the agent the pairing code was worked out with, and nobody in between.
    if (!agentFingerprint || fingerprintIn(answer) !== agentFingerprint) {
      throw new Error(t("The answer did not come from the computer you were paired with, so no connection was made. Reload and try again."));
    }
    await peer.setRemoteDescription({ type: "answer", sdp: answer });
  } catch (error) {
    fail(peer, error instanceof Error ? error.message : t("The connection could not be set up."));
  }
}
