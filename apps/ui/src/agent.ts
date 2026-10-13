import { nextTick, onMounted, onUnmounted, ref } from "vue";
import type { LocalState } from "@light-assist/protocol";
import { language } from "./i18n.js";

export const state = ref<LocalState>();
export const online = ref(false);

/**
 * The agent only acts for the app's own window, which it starts with a key in the address. The same
 * page opened in an ordinary browser has no key, and so can do nothing.
 */
const key = new URLSearchParams(location.hash.slice(1)).get("key") ?? "";

/**
 * Whether the window this page is in has no title bar of the system's, so that the page's own header
 * is its title bar, with the buttons to minimize and close it (on Windows). Said by the app in the address.
 */
export const ownTitleBar = new URLSearchParams(location.hash.slice(1)).get("titlebar") === "own";

/** Asks the app's window for what a title bar does: to be dragged, minimized or closed. */
export function windowAction(action: "ready" | "drag" | "minimize" | "close") {
  (window as unknown as { ipc?: { postMessage(message: string): void } }).ipc?.postMessage(action);
}

/** A request to the agent on this computer. */
export function api(path: string, init: RequestInit = {}) {
  return fetch(path, { ...init, headers: { ...init.headers, "x-light-assist-key": key } });
}

export async function refresh() {
  try {
    const response = await api("/api/state");
    if (!response.ok) throw new Error(`State request failed: ${response.status}`);
    state.value = (await response.json()) as LocalState;
    online.value = true;
  } catch {
    online.value = false;
  }
}

export async function post<T = unknown>(path: string, body?: unknown): Promise<T | undefined> {
  try {
    const response = await api(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: body === undefined ? null : JSON.stringify(body),
    });
    const result = (await response.json()) as T;
    await refresh();
    return result;
  } catch {
    online.value = false;
    return undefined;
  }
}

/**
 * Tells the app's window that the page is laid out, with what the agent said, so that the window is
 * shown then and not blank before. Once Vue has put the state on the page: a hidden window does not
 * draw, so there is no painting to wait for, and the first one when it is shown has everything.
 */
function announceReady() {
  void nextTick(() => windowAction("ready"));
}

/** Polls the local agent for as long as the calling component is mounted. */
export function useAgent() {
  let timer: number | undefined;

  onMounted(() => {
    void refresh().then(announceReady);
    timer = window.setInterval(refresh, 1500);
  });

  onUnmounted(() => window.clearInterval(timer));
}

/** A newer version of the app than this one, once the website has said there is one. */
export const newerVersion = ref<string | null>(null);
/** When the website was last asked, and whether it answered. */
export const updateChecked = ref<{ at: number; ok: boolean } | null>(null);

/** Whether version `a` comes after `b`, as `1.10.0` comes after `1.9.2`. */
export function isNewer(a: string, b: string) {
  const parts = (version: string) => version.split(/[.-]/).map((part) => Number.parseInt(part, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  }
  return false;
}

const UPDATE_CHECK_EVERY_MS = 6 * 60 * 60_000;
let checking: number | undefined;

/**
 * Asks the website which version of the app is the newest, now and every six hours while the console
 * is open. Not for a copy from the Microsoft Store, which brings its own updates, nor for one built
 * without a website.
 */
export async function checkForUpdates() {
  const now = state.value;
  if (!now?.website || now.installation === "store") return;
  window.clearTimeout(checking);
  checking = window.setTimeout(checkForUpdates, UPDATE_CHECK_EVERY_MS);
  try {
    const published = (await (await fetch(`${now.website}/version.json`, { cache: "no-store" })).json()) as { version?: unknown };
    const latest = typeof published.version === "string" ? published.version : null;
    newerVersion.value = latest && isNewer(latest, now.version) ? latest : null;
    updateChecked.value = { at: Date.now(), ok: true };
  } catch {
    updateChecked.value = { at: Date.now(), ok: false };
  }
}

/** Opens one of the app's pages on the website in the person's own browser. */
export function openWebsite(page: "download" | "privacy" | "terms" | "source") {
  // The policy and the terms each have a page in every language the app has.
  const localized = (page === "privacy" || page === "terms") && language.value !== "en" ? `${page}-${language.value}` : page;
  void post("/api/open", { page: localized });
}
