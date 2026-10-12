import { ref } from "vue";
import { hosted, room } from "./hosted.js";

/**
 * This browser as a trusted device of the sharer's computer.
 *
 * The sharer can trust a helper's browser during an ordinary session. It is then given a secret,
 * which it keeps for this address only, and with which it may later connect while nobody is at the
 * sharer's computer. The secret itself is never sent again: the sharer's agent sends a number, and
 * the answer is a keyed hash of it (HMAC-SHA-256).
 *
 * The helper page is not a secure context, so the browser's own cryptography is not available to it,
 * and the hash is written out here.
 */

/**
 * A browser keeps this per site. The embedded page is one sharer's computer; the hosted page is every
 * sharer's, so there the secret is kept under the name of the one it belongs to.
 */
const STORED = hosted ? `light-assist.device:${room() ?? ""}` : "light-assist.device";

interface Device {
  id: string;
  secret: string;
  /** What the sharer called this browser. */
  name: string;
}

function stored(): Device | null {
  try {
    const device = JSON.parse(localStorage.getItem(STORED) ?? "null") as Partial<Device> | null;
    if (device && /^[0-9a-f]{1,64}$/.test(device.id ?? "") && /^[0-9a-f]{64}$/.test(device.secret ?? "")) {
      return { id: device.id!, secret: device.secret!, name: String(device.name ?? "") };
    }
  } catch {
    // Nothing usable is kept.
  }
  return null;
}

/** The name the sharer gave this browser, while it is one of their trusted devices. */
export const trustedAs = ref<string | null>(stored()?.name ?? null);
/** The sharer has just made this browser a trusted device. */
export const justTrusted = ref(false);

export function deviceId() {
  return stored()?.id ?? null;
}

/** Keeps what the sharer's agent handed over when the sharer trusted this browser. */
export function rememberDevice(message: Record<string, unknown>) {
  const device = { id: String(message.id), secret: String(message.secret), name: String(message.name ?? "") };
  if (!/^[0-9a-f]{1,64}$/.test(device.id) || !/^[0-9a-f]{64}$/.test(device.secret)) return;
  try {
    localStorage.setItem(STORED, JSON.stringify(device));
    trustedAs.value = device.name;
    justTrusted.value = true;
  } catch {
    // A browser that keeps nothing cannot be a trusted device.
  }
}

/** Lets go of the secret: this browser connects like any other from now on. */
export function forgetDevice() {
  try {
    localStorage.removeItem(STORED);
  } catch {
    // There was nothing kept.
  }
  trustedAs.value = null;
  justTrusted.value = false;
}

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74,
  0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d,
  0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e,
  0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
  0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

const rotate = (value: number, by: number) => (value >>> by) | (value << (32 - by));

export function sha256(message: Uint8Array) {
  const state = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
  // The message, a single set bit, zeros, and the message's length in bits, to a whole number of blocks.
  const padded = new Uint8Array((((message.length + 8) >> 6) + 1) << 6);
  padded.set(message);
  padded[message.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Math.floor(message.length / 0x20000000));
  view.setUint32(padded.length - 4, (message.length << 3) >>> 0);

  const w = new Uint32Array(64);
  for (let block = 0; block < padded.length; block += 64) {
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(block + i * 4);
    for (let i = 16; i < 64; i++) {
      const s0 = rotate(w[i - 15]!, 7) ^ rotate(w[i - 15]!, 18) ^ (w[i - 15]! >>> 3);
      const s1 = rotate(w[i - 2]!, 17) ^ rotate(w[i - 2]!, 19) ^ (w[i - 2]! >>> 10);
      w[i] = (w[i - 16]! + s0 + w[i - 7]! + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, h] = state as unknown as [number, number, number, number, number, number, number, number];
    for (let i = 0; i < 64; i++) {
      const t1 = (h + (rotate(e, 6) ^ rotate(e, 11) ^ rotate(e, 25)) + ((e & f) ^ (~e & g)) + K[i]! + w[i]!) >>> 0;
      const t2 = ((rotate(a, 2) ^ rotate(a, 13) ^ rotate(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      [h, g, f, e, d, c, b, a] = [g, f, e, (d + t1) >>> 0, c, b, a, (t1 + t2) >>> 0];
    }
    [a, b, c, d, e, f, g, h].forEach((value, i) => (state[i] = (state[i]! + value) >>> 0));
  }
  const out = new Uint8Array(32);
  state.forEach((word, i) => new DataView(out.buffer).setUint32(i * 4, word));
  return out;
}

export function hmacSha256(key: Uint8Array, message: Uint8Array) {
  const block = new Uint8Array(64);
  block.set(key.length > 64 ? sha256(key) : key);
  const pad = (with_: number, then: Uint8Array) => {
    const all = new Uint8Array(64 + then.length);
    all.set(block.map((byte) => byte ^ with_));
    all.set(then, 64);
    return all;
  };
  return sha256(pad(0x5c, sha256(pad(0x36, message))));
}

export const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
const unhex = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((pair) => parseInt(pair, 16)));

/**
 * The answer to the sharer's agent's challenge, or nothing if this browser is not a trusted device.
 * It covers both ends' certificates, so it is good for the connection between them and no other.
 */
export function prove(nonce: string, agentFingerprint: string, ownFingerprint: string) {
  const device = stored();
  const challenge = `${nonce}|${agentFingerprint}|${ownFingerprint}`;
  return device ? hex(hmacSha256(unhex(device.secret), new TextEncoder().encode(challenge))) : null;
}
