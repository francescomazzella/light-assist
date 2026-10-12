import { PROTOCOL } from "@light-assist/protocol";
import { hex, sha256 } from "./device.js";
import { t } from "./i18n.js";

/**
 * The helper page's half of working out the pairing code (the agent's is `pairing.rs`, where the
 * exchange is described).
 *
 * The code is a digest of both ends' encryption certificates and a random number from each, so
 * someone sitting between the two cannot make both arrive at the same one. The page makes its
 * certificate before there is a connection, and later accepts the connection only from the
 * certificate the agent named here.
 */

const utf8 = (text: string) => new TextEncoder().encode(text);
const digest = (text: string) => sha256(utf8(text));

function randomHex(bytes: number) {
  return hex(crypto.getRandomValues(new Uint8Array(bytes)));
}

/** The fingerprint an SDP offer or answer names for its end, as `AB:CD:…`. */
export function fingerprintIn(sdp: string) {
  return /^a=fingerprint:sha-256 (\S+)/im.exec(sdp)?.[1]?.toUpperCase() ?? null;
}

let identity: Promise<{ certificate: RTCCertificate; fingerprint: string }> | undefined;

/** This page's certificate, the same for as long as the page is open, and its fingerprint. */
export function ownIdentity() {
  identity ??= (async () => {
    const certificate = await RTCPeerConnection.generateCertificate({ name: "ECDSA", namedCurve: "P-256" } as AlgorithmIdentifier);
    // Asked of a connection that goes nowhere: what it would offer names the certificate.
    const probe = new RTCPeerConnection({ certificates: [certificate] });
    probe.createDataChannel("probe");
    const fingerprint = fingerprintIn((await probe.createOffer()).sdp ?? "");
    probe.close();
    if (!fingerprint) throw new Error(t("This browser did not say which certificate it uses."));
    return { certificate, fingerprint };
  })();
  return identity;
}

/** The agent's certificate, once it has named it: the only one the connection is accepted from. */
export let agentFingerprint: string | null = null;

let ownNonce = "";

/** Begins a pairing code: what the page binds itself to before it is told the agent's number. */
export async function commit() {
  agentFingerprint = null;
  ownNonce = randomHex(16);
  const { fingerprint } = await ownIdentity();
  return { type: "commit" as const, protocol: PROTOCOL, commit: hex(digest(`${ownNonce}|${fingerprint}`)) };
}

/** Finishes it: what the page then reveals, and the six digits to read out. */
export async function reveal(agent: { nonce: string; fingerprint: string }) {
  const { fingerprint } = await ownIdentity();
  agentFingerprint = agent.fingerprint;
  const bytes = digest(`${fingerprint}|${agent.fingerprint}|${ownNonce}|${agent.nonce}`);
  const number = new DataView(bytes.buffer).getUint32(0) % 1_000_000;
  return { message: { type: "reveal" as const, nonce: ownNonce, fingerprint }, code: String(number).padStart(6, "0") };
}

/** For a trusted device, which is let in without a code: the agent's certificate, from its challenge. */
export function expectAgent(fingerprint: string) {
  agentFingerprint = fingerprint;
}
