import { t } from "./i18n.js";

/**
 * The helper page, hosted.
 *
 * The same page is built twice. Embedded in the sharer's app, it is served by that computer and talks
 * to it directly. Hosted on a website, over HTTPS, it serves every sharer: the address says which one
 * (`https://assist.example.com/maple-river`), and signaling goes through the relay to a room of that name.
 */

/** The relay's address, set when the page is built for hosting and absent from the embedded page. */
const relay = ((import.meta.env.VITE_RELAY_URL as string | undefined) ?? "").replace(/\/$/, "");

export const hosted = relay !== "";

const ROOM = /^[a-z]{2,10}(-[a-z]{2,10}){1,3}$/;

/** What someone typed or was sent, as a room's name: `Maple River`, `maple-river` and `assist-maple-river.local` are one. */
export function roomNamed(words: string) {
  const name = words
    .trim()
    .toLowerCase()
    .replace(/^assist-|\.local$/g, "")
    .replace(/[^a-z]+/g, "-")
    .replace(/^-|-$/g, "");
  return ROOM.test(name) ? name : null;
}

/** The sharer this page was opened for, from its address. `null` on the embedded page, and when the address names none. */
export function room() {
  return hosted ? roomNamed(decodeURIComponent(location.pathname.split("/")[1] ?? "")) : null;
}

/** Where this page's signaling socket goes. */
export function signalingUrl(device: string | null) {
  const asDevice = device ? `?device=${device}` : "";
  if (hosted) return `${relay.replace(/^http/, "ws")}/helper/${room()}${asDevice}`;
  return `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws${asDevice}`;
}

/**
 * Where to get the app, for someone who reached the website wanting assistance and not to give it.
 * The files are the ones attached to the latest release, under names that do not change from one
 * version to the next; the Store's address is set once there is a listing (`VITE_STORE_URL`).
 */
const releases = ((import.meta.env.VITE_DOWNLOADS_URL as string | undefined) ?? "https://github.com/francescomazzella/light-assist/releases/latest/download").replace(/\/$/, "");
const store = (import.meta.env.VITE_STORE_URL as string | undefined) ?? "";

/** Named when they are shown, so that they are in the reader's language. */
export const downloads = (): { name: string; note: string; url: string }[] => [
  ...(store ? [{ name: "Microsoft Store", note: t("Windows, kept up to date"), url: store }] : []),
  { name: "Windows", note: store ? t("Without the Store") : t("Windows 10 and 11"), url: `${releases}/LightAssist-Windows.exe` },
  { name: "Mac", note: t("macOS 13 or later, Apple silicon"), url: `${releases}/LightAssist-macOS.zip` },
];

/** This page's build, to tell it from a newer one on the website. */
const build = (import.meta.env.VITE_BUILD as string | undefined) ?? "";
const CHECK_EVERY_MS = 10 * 60_000;

/**
 * Keeps a hosted page that stays open from going out of date: when a newer one has been published, it
 * loads it the next time `idle()` says nothing would be lost by doing so, such as a code being read
 * out or a session under way. Looks when the page comes back into view, and every ten minutes.
 */
export function stayUpToDate(idle: () => boolean) {
  if (!hosted || !build) return;
  let newer = false;
  const reloadIfIdle = () => {
    if (newer && idle()) location.reload();
  };
  const look = async () => {
    try {
      const published = (await (await fetch("/version.json", { cache: "no-store" })).json()) as { build?: string };
      newer = typeof published.build === "string" && published.build !== build;
    } catch {
      // The website could not be reached; the next look tries again.
    }
    reloadIfIdle();
  };
  window.setInterval(look, CHECK_EVERY_MS);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") void look();
  });
  return reloadIfIdle;
}
