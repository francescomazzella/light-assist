import { ref } from "vue";

/**
 * The helper's end of clipboard sharing.
 *
 * The helper page is served over plain HTTP, where a browser gives a page no clipboard of its own to
 * call. What it does allow is enough: a paste the helper makes on the page carries the text with it,
 * and a page may copy something while the helper is clicking or typing on it.
 */

/** The most text that travels either way, in bytes; the sharer's agent has the same limit. */
const MAX_TEXT_BYTES = 100_000;
const NOTICE_MS = 2500;

export type ClipboardNotice =
  /** Text copied on the sharer's computer is now on this one's clipboard. */
  | "copied"
  /** It is here, but the browser will only take it during a click or a key press on the page. */
  | "waiting"
  /** What was copied on the sharer's computer was too much to bring over. */
  | "too-large"
  /** What the helper tried to paste was too much to send. */
  | "paste-too-large"
  | null;
export const clipboardNotice = ref<ClipboardNotice>(null);

let waiting: string | undefined;
let noticeTimer: number | undefined;

function notify(notice: ClipboardNotice, lasting = false) {
  window.clearTimeout(noticeTimer);
  clipboardNotice.value = notice;
  if (notice && !lasting) noticeTimer = window.setTimeout(() => (clipboardNotice.value = null), NOTICE_MS);
}

export function fitsClipboard(text: string) {
  // A character is at most three bytes for each unit of a string's length, so short text need not be measured.
  return text.length * 3 <= MAX_TEXT_BYTES || new TextEncoder().encode(text).length <= MAX_TEXT_BYTES;
}

export function pasteTooLarge() {
  notify("paste-too-large");
}

/** Puts `text` on this computer's clipboard, if the browser lets the page do so right now. */
function write(text: string) {
  // The page is asked for what to copy, and answers with the text.
  let answered = false;
  const answer = (event: ClipboardEvent) => {
    event.clipboardData?.setData("text/plain", text);
    event.preventDefault();
    event.stopImmediatePropagation();
    answered = true;
  };
  document.addEventListener("copy", answer, true);
  let accepted = false;
  try {
    accepted = document.execCommand("copy");
  } catch {
    // Refused; the text waits for the next click or key press.
  }
  document.removeEventListener("copy", answer, true);
  if (accepted && answered) return true;

  // Some browsers only copy a selection, so give them one.
  const holder = document.createElement("textarea");
  holder.value = text;
  holder.setAttribute("readonly", "");
  holder.style.cssText = "position:fixed;top:0;left:0;width:1px;height:1px;opacity:0";
  const focused = document.activeElement;
  document.body.append(holder);
  holder.select();
  try {
    accepted = document.execCommand("copy");
  } catch {
    accepted = false;
  }
  holder.remove();
  if (focused instanceof HTMLElement) focused.focus();
  return accepted;
}

/** Copies whatever is waiting. Called during every click and key press, which is when a browser allows it. */
export function copyWaiting() {
  if (waiting === undefined || !write(waiting)) return;
  waiting = undefined;
  notify("copied");
}

/**
 * On a secure page (the hosted one) the browser has a clipboard a page may write to, in some browsers
 * without waiting for a click. Returns whether it took the text.
 */
async function writeDirectly(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Not a secure page, not in front, or a browser that wants a click first.
    return false;
  }
}

/** Text was copied on the sharer's computer; `null` when it was too much to send. */
export async function receiveClipboard(text: string | null) {
  if (text === null) return notify("too-large");
  waiting = text;
  if (await writeDirectly(text)) {
    // Unless something newer arrived meanwhile, which is then the one that waits.
    if (waiting === text) waiting = undefined;
    return notify("copied");
  }
  if (waiting !== text) return;
  copyWaiting();
  // The helper has not touched the page for a while, as when it was the sharer who copied.
  if (waiting !== undefined) notify("waiting", true);
}

export function forgetClipboard() {
  waiting = undefined;
  notify(null);
}
