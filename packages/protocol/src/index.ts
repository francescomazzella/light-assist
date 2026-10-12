/**
 * Which version of the signaling messages this is. The agent and the helper page each say theirs, and
 * each tells its user when the other is one it cannot work with. Mirrored in `pairing.rs`.
 */
export const PROTOCOL = 2;

/** "view" is screen only; "control" adds mouse and keyboard. */
export type AccessLevel = "view" | "control";

/** This computer is asking for help and can be reached on the LAN. */
export interface HelpRequestInfo {
  /**
   * The hosted helper page at this help request's name, while the relay can be reached. It is the
   * address to give out when there is one; the others are for when it does not load.
   */
  hostedAddress: string | null;
  /** The same address while the relay cannot be reached: it does not work right now, and will again. */
  hostedOffline: string | null;
  /** The help address the helper opens in a browser. */
  address: string;
  /** The same by IP address, for networks where `.local` names do not resolve. */
  fallbackAddress: string | null;
  /** The address for a helper who shares a Tailscale network with this computer, if it is on one. */
  tailscaleAddress: string | null;
  expiresAt: number;
  /** Helper pages currently showing a pairing code. */
  waiting: number;
}

export interface SessionInfo {
  helperAddress: string;
  /** The helper is using the hosted helper page, and reached this computer through the relay. */
  hosted: boolean;
  startedAt: number;
  access: AccessLevel;
  /** The helper asked for control and the sharer must allow or deny it. */
  controlRequested: boolean;
  /** The terminal grant: separate from the access level, and never implied by it. */
  terminal: boolean;
  terminalRequested: boolean;
  /** The connection to the helper is too slow for a good picture. */
  slowConnection: boolean;
  /** Whether text copied on either computer is shared while the access level is control. */
  clipboard: boolean;
  /** The trusted device that connected by itself, with nobody there to let it in: its name. */
  unattended: string | null;
  /** Whether the direct connection between the two computers is up. */
  linked: boolean;
  /** The direct connection was up and has been lost; the session ends by itself if it stays down. */
  linkLost: boolean;
  /** Whether this helper's browser may be made a trusted device. */
  canTrust: boolean;
}

/** A helper's browser that may connect while nobody is at the sharer's computer. */
export interface DeviceInfo {
  id: string;
  /** What the sharer called it. */
  name: string;
  addedAt: number;
  lastUsed: number | null;
}

/** Unattended access as the console shows it, present while the sharer has it switched on. */
export interface UnattendedInfo {
  /** Where a trusted device connects, which stays the same from one day to the next. */
  address: string | null;
  fallbackAddress: string | null;
  /** The address for a trusted device on a Tailscale network this computer is on. */
  tailscaleAddress: string | null;
  hostedAddress: string | null;
  hostedOffline: string | null;
  /** Whether this computer can be reached right now. */
  listening: boolean;
  devices: DeviceInfo[];
}

/**
 * What the sharer's system has been told to let the app do. Only a Mac asks its user such things, in
 * System Settings, and nothing the sharer allows in the app works until they have.
 */
export interface Permissions {
  /** Seeing the screen, and hearing what the computer plays. */
  screen: boolean;
  /** Using the mouse and keyboard. */
  control: boolean;
}

/** Everything the loopback-only console renders. */
export interface LocalState {
  address: string;
  /** What kind of computer this is. */
  platform: "windows" | "macos" | "linux";
  /** This app's version, such as `0.1.0`. */
  version: string;
  /** The website, if this app was built to know one: where updates, downloads and the privacy policy are. */
  website: string | null;
  /** How this copy was installed, which decides who brings its updates. */
  installation: AppInfo["installation"];
  /** Absent where the system asks its user for nothing. */
  permissions: Permissions | null;
  unattended: UnattendedInfo | null;
  request: HelpRequestInfo | null;
  session: SessionInfo | null;
  /** Why the last session ended by itself, until the next help request starts. */
  ended: Ended | null;
  /** The keyboard shortcut that ends a session from anywhere, as it should be shown. */
  stopKey: string;
}

/**
 * A reason for a session to end without either person ending it: a helper with control or a terminal
 * did nothing for too long, the session reached the longest time the sharer allows, or the direct
 * connection between the two computers was lost and did not come back.
 */
export type Ended = "idle" | "time-limit" | "connection-lost";

/** How the app is installed, and whether it starts with Windows. */
export interface AppInfo {
  /** From the Microsoft Store, installed by its own download, or run from wherever it was downloaded to. */
  installation: "store" | "installed" | "portable";
  startWithWindows: boolean;
  /** Why start-with-Windows cannot be changed from the app, when it cannot. */
  startupNote: string | null;
}

/** The sharer's preferences. A value of 0 means never. */
export interface Settings {
  /** End the session when a helper with control or a terminal has done nothing for this long. */
  idleMinutes: number;
  /** End any session after this long. */
  maxMinutes: number;
  /** Share copied text, both ways, with a helper who has control. */
  clipboard: boolean;
  /** Let trusted devices on the local network connect and take control while nobody is there. */
  unattended: boolean;
}

export type Failure = "invalid-code" | "bad-request" | "not-asking" | "busy" | "port-unavailable" | "not-active" | "version" | "not-trusted";

/** Reply to the console's requests that can fail. */
export type Outcome = { ok: true } | { ok: false; reason: Failure };

/** Signaling messages from the helper page. */
export type HelperMessage =
  | { type: "request-control" }
  | { type: "request-terminal" }
  | { type: "offer"; sdp: string }
  /** The page binds itself to its random number and certificate, before it is told the agent's. */
  | { type: "commit"; protocol: number; commit: string }
  /** The page's random number and certificate fingerprint, which must match what it committed to. */
  | { type: "reveal"; nonce: string; fingerprint: string }
  /**
   * A trusted device's answer to the challenge: the keyed hash, under the device's secret, of the
   * challenge and both ends' certificate fingerprints. With it, the page's own fingerprint.
   */
  | { type: "proof"; proof: string; fingerprint: string };

/**
 * Messages from the helper on the `terminal` data channel, honored only with the terminal grant.
 * The shell's output travels the other way as raw bytes, and `{ type: "closed" }` when the shell is gone.
 */
export type TerminalMessage =
  /** The size of the helper's terminal in character cells. The first one starts the shell. */
  | { type: "resize"; cols: number; rows: number }
  | { type: "stdin"; data: string };

export type MouseButton = "left" | "middle" | "right";

/** Messages on the `input` data channel, honored only while the access level is control. */
export type InputMessage =
  /** A position on the shared screen, as fractions of its width and height. */
  | { type: "mouse-move"; x: number; y: number }
  | { type: "mouse-down" | "mouse-up"; button: MouseButton }
  /** Wheel notches; positive is right and down. */
  | { type: "wheel"; dx: number; dy: number }
  /** A physical key, as `KeyboardEvent.code` names it (such as `KeyC` or `ArrowUp`). */
  | { type: "key-down" | "key-up"; code: string }
  /** Typed characters, which arrive as they are whatever the two keyboard layouts. */
  | { type: "text"; text: string }
  /** Text the helper is about to paste: it goes onto the sharer's clipboard, and the paste follows as keys. */
  | { type: "clipboard"; text: string };

/** Signaling messages to the helper page. */
export type AgentMessage =
  /** The first thing a page is told, and told again when its pairing code has grown old: begin one. */
  | { type: "hello"; protocol: number }
  /** The agent's random number and certificate fingerprint, in answer to the page's commitment. */
  | { type: "pairing"; nonce: string; fingerprint: string }
  /** What agents before version 2 sent instead: a code of their own choosing. */
  | { type: "code"; code: string }
  /** To a page that says it is a trusted device: prove it. With the agent's certificate fingerprint. */
  | { type: "challenge"; nonce: string; fingerprint: string }
  | { type: "admitted" }
  | { type: "access"; access: AccessLevel }
  | { type: "control-denied" }
  /** The sharer allowed the terminal, refused it, or closed it again. */
  | { type: "terminal"; allowed: boolean }
  | { type: "answer"; sdp: string }
  | { type: "rtc-failed" }
  | { type: "closed"; reason?: Ended }
  | { type: "error"; reason: Failure };
