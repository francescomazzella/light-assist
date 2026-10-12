import { ref } from "vue";
import { PROTOCOL, type AccessLevel, type AgentMessage, type Failure, type HelperMessage } from "@light-assist/protocol";
import { deviceId, prove } from "./device.js";
import { hosted, room, signalingUrl } from "./hosted.js";
import { t } from "./i18n.js";
import { closeLink, openLink } from "./link.js";
import { commit, expectAgent, ownIdentity, reveal } from "./pairing.js";

/** "which" is the hosted page opened without saying whose computer: it asks. */
export type Phase = "which" | "connecting" | "code" | "admitted" | "ended";

export const phase = ref<Phase>("connecting");
export const code = ref("");
export const access = ref<AccessLevel>("view");
export const control = ref<"idle" | "requested" | "denied">("idle");
/** The terminal grant, which the sharer gives and takes back separately from the access level. */
export const terminal = ref<"idle" | "requested" | "denied" | "granted">("idle");
export const ended = ref("");

const endedText = (reason: Failure) =>
  ({
    "not-asking": t("This computer is not requesting assistance right now."),
    busy: t("Someone else is already assisting, or too many people are waiting."),
    "invalid-code": t("That code is not valid."),
    "bad-request": t("The other computer did not understand this page. Reload and try again."),
    "port-unavailable": t("The other computer could not be reached."),
    "not-active": t("The session has not been allowed yet."),
    version: t("This page and Light Assist on the other computer are different versions. Reload this page; if that does not help, they need to update Light Assist."),
    "not-trusted": t("The other computer no longer lets this browser connect by itself."),
  })[reason];

const olderApp = () => t("Light Assist on the other computer is an older version that this page can no longer work with. They need to update it.");
const newerApp = () => t("Light Assist on the other computer is newer than this page. Reload the page and try again.");

const RTC_TIMEOUT_MS = 15_000;

let socket: WebSocket | undefined;
let pendingAnswer: { resolve: (sdp: string) => void; reject: (error: Error) => void } | undefined;

function send(message: HelperMessage) {
  if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
}

function settle(result: { sdp: string } | { error: string }) {
  const pending = pendingAnswer;
  pendingAnswer = undefined;
  if (!pending) return;
  if ("sdp" in result) pending.resolve(result.sdp);
  else pending.reject(new Error(result.error));
}

/** The SDP offer travels over the same socket the pairing code arrived on. */
function signal(sdp: string) {
  return new Promise<string>((resolve, reject) => {
    settle({ error: t("A newer connection attempt replaced this one.") });
    pendingAnswer = { resolve, reject };
    send({ type: "offer", sdp });
    window.setTimeout(() => {
      if (pendingAnswer?.resolve === resolve) settle({ error: t("The other computer did not answer in time.") });
    }, RTC_TIMEOUT_MS);
  });
}

function end(message: string) {
  if (phase.value === "ended") return;
  phase.value = "ended";
  ended.value = message;
  settle({ error: message });
  closeLink();
  socket?.close();
}

async function receive(message: AgentMessage) {
  switch (message.type) {
    case "hello":
      // Also sent when a code has grown old: a new one is worked out, and the old one goes.
      if (message.protocol !== PROTOCOL) return end(message.protocol < PROTOCOL ? olderApp() : newerApp());
      if (phase.value === "code") phase.value = "connecting";
      send(await commit());
      break;
    case "pairing": {
      const paired = await reveal(message);
      send(paired.message);
      code.value = paired.code;
      phase.value = "code";
      break;
    }
    // A code the agent chose by itself, which proves nothing about who is in between.
    case "code":
      end(olderApp());
      break;
    case "challenge": {
      if (typeof message.fingerprint !== "string") return end(olderApp());
      const { fingerprint } = await ownIdentity();
      expectAgent(message.fingerprint);
      const proof = prove(message.nonce, message.fingerprint, fingerprint);
      if (proof) send({ type: "proof", proof, fingerprint });
      break;
    }
    case "admitted":
      phase.value = "admitted";
      access.value = "view";
      void openLink(signal);
      break;
    case "access":
      access.value = message.access;
      control.value = "idle";
      break;
    case "control-denied":
      control.value = "denied";
      break;
    case "terminal":
      // A "no" answers a request; without one pending, it means the sharer closed an open terminal.
      if (message.allowed) terminal.value = "granted";
      else terminal.value = terminal.value === "requested" ? "denied" : "idle";
      break;
    case "answer":
      settle({ sdp: message.sdp });
      break;
    case "rtc-failed":
      settle({ error: t("The other computer could not set up the connection.") });
      break;
    case "closed":
      if (message.reason === "idle") end(t("The session has ended, because nothing was done with it for a while."));
      else if (message.reason === "time-limit") end(t("The session has ended: it reached the time limit set on the other computer."));
      else if (message.reason === "connection-lost") end(t("The session has ended, because the connection between the two computers was lost."));
      else end(t("The session has ended."));
      break;
    case "error":
      // A refused SDP offer is not the end of the session.
      if (message.reason === "not-active") settle({ error: endedText(message.reason) });
      // Not trusted any more, or not on their network just now: connect as anyone would, for a code.
      else if ((message.reason === "not-trusted" || message.reason === "not-asking") && asDevice) connect(false);
      else end(endedText(message.reason));
      break;
  }
}

/** Whether the current socket was opened as a trusted device. */
let asDevice = false;

/** Connects to the sharer's agent: as a trusted device when this browser is one, unless told not to. */
export function connect(trusted = true) {
  if (hosted && !room()) {
    phase.value = "which";
    return;
  }
  const device = trusted === false ? null : deviceId();
  asDevice = device !== null;
  phase.value = "connecting";
  control.value = "idle";
  terminal.value = "idle";
  ended.value = "";

  const opened = new WebSocket(signalingUrl(device));
  socket = opened;
  // One at a time and in the order they came, though working a code out takes a moment.
  let handled = Promise.resolve();
  opened.onmessage = (event) => {
    handled = handled
      .then(() => (socket === opened ? receive(JSON.parse(String(event.data)) as AgentMessage) : undefined))
      .catch((error) => {
        // Not a signaling message, or a browser that cannot make a certificate.
        if (error instanceof Error && !(error instanceof SyntaxError)) end(error.message);
      });
  };
  opened.onclose = () => {
    if (socket === opened) end(t("This computer is no longer requesting assistance, or the connection was lost."));
  };
}

export function requestControl() {
  control.value = "requested";
  send({ type: "request-control" });
}

export function requestTerminal() {
  terminal.value = "requested";
  send({ type: "request-terminal" });
}

export function retryLink() {
  void openLink(signal);
}

export function leave() {
  end(t("You disconnected."));
}
