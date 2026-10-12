import { ref, watch } from "vue";
import type { MouseButton } from "@light-assist/protocol";
import { fitsClipboard, pasteTooLarge } from "./clipboard.js";
import { sendInput, sharerPlatform } from "./link.js";

/** Whether this helper is on a Mac, whose Command key has no counterpart on the sharer's keyboard. */
export const hasCommandKey = /Mac|iPhone|iPad/.test(navigator.platform);

const COMMAND_SETTING = "light-assist.command-as-control";
const CONTROL_SETTING = "light-assist.control-as-command";
function remembered(setting = COMMAND_SETTING) {
  try {
    return localStorage.getItem(setting) !== "off";
  } catch {
    return true;
  }
}

/**
 * The helper's choice: Command with one of the everyday shortcut keys acts as Ctrl on the sharer's
 * computer, so that Command+C copies as a Mac user expects. Any other use of Command stays the
 * Windows key.
 */
export const commandAsControl = ref(remembered());
watch(commandAsControl, (on) => {
  try {
    localStorage.setItem(COMMAND_SETTING, on ? "on" : "off");
  } catch {
    // The choice simply lasts for this visit.
  }
});

/**
 * The same choice the other way round, for a helper without a Command key whose sharer is on a Mac:
 * Ctrl with one of the everyday shortcut keys acts as Command there, so that Ctrl+C copies. Any other
 * use of Ctrl stays Control.
 */
export const controlAsCommand = ref(remembered(CONTROL_SETTING));
watch(controlAsCommand, (on) => {
  try {
    localStorage.setItem(CONTROL_SETTING, on ? "on" : "off");
  } catch {
    // The choice simply lasts for this visit.
  }
});

/** Copy, paste, cut, select all, undo, redo, save and find. */
const COMMAND_SHORTCUTS = new Set(["KeyC", "KeyV", "KeyX", "KeyA", "KeyZ", "KeyY", "KeyS", "KeyF"]);
const isCommand = (code: string) => code === "MetaLeft" || code === "MetaRight";
const CONTROLS = ["ControlLeft", "ControlRight"];

/** How long a paste shortcut waits for the browser's paste. It follows at once, or not at all. */
const PASTE_WAIT_MS = 150;

const BUTTONS: Record<number, MouseButton> = { 0: "left", 1: "middle", 2: "right" };
/** Browsers report a wheel notch as about 100 pixels, 3 lines or 1 page. */
const NOTCHES_PER_UNIT = [1 / 100, 1 / 3, 1];

const clamp = (value: number) => Math.min(1, Math.max(0, value));

/**
 * Key combinations the helper's browser or computer keeps for itself, offered as buttons instead.
 * Each is a list of key codes, and which there are depends on what the sharer's computer is.
 */
export const SPECIAL_KEYS: Record<"windows" | "macos", Record<string, readonly string[]>> = {
  windows: {
    Win: ["MetaLeft"],
    "Alt+Tab": ["AltLeft", "Tab"],
    Esc: ["Escape"],
  },
  macos: {
    "⌘ Space": ["MetaLeft", "Space"],
    "⌘ Tab": ["MetaLeft", "Tab"],
    Esc: ["Escape"],
  },
};

/** Presses the keys in order and lets go of them in reverse, as a person would. */
export function pressKeys(codes: readonly string[]) {
  for (const code of codes) sendInput({ type: "key-down", code });
  for (const code of [...codes].reverse()) sendInput({ type: "key-up", code });
}

/**
 * Forwards the keyboard to the sharer's computer. Returns a function that stops doing so.
 *
 * Ordinary typing is sent as the characters themselves, so it comes out right whatever layout each
 * keyboard has. Everything else (shortcuts, arrows, Enter, modifiers) is sent as physical keys.
 *
 * A paste carries the helper's clipboard with it: the text goes first, onto the sharer's clipboard,
 * and the keys that paste it there follow.
 */
export function forwardKeyboard() {
  const held = new Set<string>();
  const toMac = () => sharerPlatform.value === "macos";
  /** Ctrl is down here but was let go on the sharer's Mac, where a shortcut needed Command by itself. */
  let controlAside = false;
  /** A paste shortcut's key press, kept back until the browser has handed over the clipboard. */
  let afterPaste: (() => void) | undefined;
  let pasteTimer = 0;
  /**
   * A Command key that is down, and what it has turned out to be. It is not sent when pressed, because
   * that cannot be taken back: a Windows key pressed and released by itself opens the Start menu.
   */
  let command: { code: string; as: "undecided" | "control" | "windows" } | undefined;

  /** Handles a key press under the Command option. Returns whether the press has been dealt with. */
  function pressedWithCommand(event: KeyboardEvent) {
    // Recognized by what the key is, not only by where it sits: keyboards differ in what they report.
    if (event.key === "Meta" || isCommand(event.code)) {
      if (!event.repeat) command = { code: isCommand(event.code) ? event.code : "MetaLeft", as: "undecided" };
      return true;
    }
    if (!command) return false;

    if (COMMAND_SHORTCUTS.has(event.code) && command.as !== "windows") {
      if (command.as === "undecided") {
        command.as = "control";
        held.add("ControlLeft");
        sendInput({ type: "key-down", code: "ControlLeft" });
      }
      // Pressed and released at once: a Mac reports no release for a key let go while Command is down.
      pressKeys([event.code]);
      return true;
    }
    if (command.as === "undecided") {
      command.as = "windows";
      held.add(command.code);
      sendInput({ type: "key-down", code: command.code });
    }
    return false;
  }

  /** Handles a key release under the Command option. Returns whether it has been dealt with. */
  function releasedCommand(event: KeyboardEvent) {
    if (!command || (event.key !== "Meta" && event.code !== command.code)) return false;
    // Command by itself is the Windows key by itself.
    if (command.as === "undecided") pressKeys([command.code]);
    else release([command.as === "control" ? "ControlLeft" : command.code]);
    command = undefined;
    return true;
  }

  function release(codes: Iterable<string>) {
    for (const code of codes) {
      if (held.delete(code)) sendInput({ type: "key-up", code });
    }
  }

  /** Handles a key press under the Ctrl option, for a sharer on a Mac. Returns whether it has been dealt with. */
  function pressedAsCommand(event: KeyboardEvent) {
    if (!event.ctrlKey || event.altKey || event.metaKey || !COMMAND_SHORTCUTS.has(event.code)) return false;
    // Control was passed on when it went down; with it still down, this would be another shortcut.
    release(CONTROLS);
    controlAside = true;
    pressKeys(["MetaLeft", event.code]);
    return true;
  }

  const isPaste = (event: KeyboardEvent) =>
    event.key.toLowerCase() === "v" && !event.altKey && (hasCommandKey ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey);

  function pasteHandled() {
    window.clearTimeout(pasteTimer);
    const press = afterPaste;
    afterPaste = undefined;
    press?.();
  }

  function onPaste(event: ClipboardEvent) {
    if (forTerminal(event)) return;
    event.preventDefault();
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (text && !fitsClipboard(text)) {
      // Pasting what the sharer's clipboard already held instead would only confuse.
      afterPaste = undefined;
      return pasteTooLarge();
    }
    // Without text (a picture, a file) the paste is of whatever the sharer's own clipboard holds.
    if (text) sendInput({ type: "clipboard", text });
    // A paste chosen from the browser's menu has no key press to follow it.
    if (afterPaste) pasteHandled();
    else pressKeys([toMac() ? "MetaLeft" : "ControlLeft", "KeyV"]);
  }

  // Keys typed into the terminal panel belong to the terminal, not to the sharer's desktop.
  const forTerminal = (event: Event) => event.target instanceof Element && event.target.closest(".terminal-panel") !== null;

  function onKeyDown(event: KeyboardEvent) {
    if (forTerminal(event)) return;
    // A dead key or an input method is still composing; the finished character follows in a later event.
    if (event.isComposing || event.key === "Dead" || event.key === "Process") return;
    pasteHandled();
    if (isPaste(event)) {
      // The browser hands over the clipboard only in the paste it makes of this very key press, so the
      // press is left to the browser and passed on once the text has gone ahead of it. Should the
      // browser make no paste of it, it is passed on as it is.
      afterPaste = () => press(event);
      pasteTimer = window.setTimeout(pasteHandled, PASTE_WAIT_MS);
      return;
    }
    event.preventDefault();
    press(event);
  }

  /** Passes a key press on to the sharer's computer. */
  function press(event: KeyboardEvent) {
    if (hasCommandKey && toMac()) {
      // Command is Command on both sides. A Mac reports no release for a key let go while Command
      // is down, so such a key is pressed and released at once.
      if (event.metaKey && event.key !== "Meta" && event.code) return pressKeys([event.code]);
    } else if (hasCommandKey) {
      if (commandAsControl.value && pressedWithCommand(event)) return;
    } else if (toMac() && controlAsCommand.value && pressedAsCommand(event)) return;

    const altGraph = event.getModifierState("AltGraph");
    const typed = [...event.key].length === 1;
    if (typed && !event.metaKey && (altGraph || (!event.ctrlKey && !event.altKey))) {
      // AltGr reaches the browser as Ctrl+Alt. Those were forwarded as keys a moment ago and must not
      // still be down on the sharer's side when the character arrives, or it would act as a shortcut.
      if (altGraph) release(["ControlLeft", "AltRight"]);
      sendInput({ type: "text", text: event.key });
      return;
    }

    if (!event.code) return;
    if (controlAside && event.ctrlKey && !CONTROLS.includes(event.code)) {
      // Ctrl has been down all along, and this key is not one that Command takes over.
      controlAside = false;
      held.add("ControlLeft");
      sendInput({ type: "key-down", code: "ControlLeft" });
    }
    held.add(event.code);
    sendInput({ type: "key-down", code: event.code });
  }

  function onKeyUp(event: KeyboardEvent) {
    if (forTerminal(event)) return;
    event.preventDefault();
    // A key released before its press was passed on would otherwise stay down.
    pasteHandled();
    if (releasedCommand(event)) return;
    if (CONTROLS.includes(event.code)) {
      // Whichever Ctrl it was, the one pressed again on its behalf goes with it.
      controlAside = false;
      return release(CONTROLS);
    }
    release([event.code]);
  }

  // Keys still down when the page loses the keyboard would never be released otherwise.
  const releaseAll = () => {
    window.clearTimeout(pasteTimer);
    afterPaste = undefined;
    command = undefined;
    controlAside = false;
    release([...held]);
  };

  window.addEventListener("keydown", onKeyDown);
  window.addEventListener("keyup", onKeyUp);
  window.addEventListener("paste", onPaste);
  window.addEventListener("blur", releaseAll);

  return () => {
    window.removeEventListener("keydown", onKeyDown);
    window.removeEventListener("keyup", onKeyUp);
    window.removeEventListener("paste", onPaste);
    window.removeEventListener("blur", releaseAll);
    releaseAll();
  };
}

/**
 * Where the pointer is on the shared screen, as fractions of its width and height. The picture is
 * letterboxed inside the element, so the bars around it are not part of the screen.
 */
function locate(video: HTMLVideoElement, event: MouseEvent) {
  if (video.videoWidth === 0 || video.videoHeight === 0) return undefined;

  const box = video.getBoundingClientRect();
  const scale = Math.min(box.width / video.videoWidth, box.height / video.videoHeight);
  const width = video.videoWidth * scale;
  const height = video.videoHeight * scale;
  return {
    x: (event.clientX - box.left - (box.width - width) / 2) / width,
    y: (event.clientY - box.top - (box.height - height) / 2) / height,
  };
}

const inside = (point: { x: number; y: number }) => point.x >= 0 && point.x <= 1 && point.y >= 0 && point.y <= 1;
/** Several frames long: it only ever matters when the frame did not come. */
const FRAME_FALLBACK_MS = 50;

/** Forwards the mouse over `video` to the sharer's computer. Returns a function that stops doing so. */
export function forwardMouse(video: HTMLVideoElement) {
  let pending: { x: number; y: number } | undefined;
  let frame = 0;
  let fallback = 0;
  const dragging = new Set<number>();

  function flush() {
    window.cancelAnimationFrame(frame);
    window.clearTimeout(fallback);
    frame = fallback = 0;
    if (pending) sendInput({ type: "mouse-move", x: clamp(pending.x), y: clamp(pending.y) });
    pending = undefined;
  }

  function onMove(event: PointerEvent) {
    const point = locate(video, event);
    // A drag keeps following the pointer past the edge of the picture; a plain move does not.
    if (!point || (!inside(point) && dragging.size === 0)) return;

    // Mice report faster than the screen refreshes; one position per frame is all that can be seen.
    // A browser skips animation frames when it decides the page need not be drawn (a headless one
    // does so now and then), so a timer stands behind each frame and sends the position if it never comes.
    pending = point;
    if (!frame) {
      frame = window.requestAnimationFrame(flush);
      fallback = window.setTimeout(flush, FRAME_FALLBACK_MS);
    }
  }

  function onDown(event: PointerEvent) {
    const button = BUTTONS[event.button];
    const point = locate(video, event);
    if (!button || !point || !inside(point)) return;

    event.preventDefault();
    video.setPointerCapture(event.pointerId);
    dragging.add(event.button);
    pending = point;
    flush();
    sendInput({ type: "mouse-down", button });
  }

  function onUp(event: PointerEvent) {
    const button = BUTTONS[event.button];
    if (!button || !dragging.delete(event.button)) return;

    const point = locate(video, event);
    if (point) pending = point;
    flush();
    sendInput({ type: "mouse-up", button });
  }

  function onWheel(event: WheelEvent) {
    const point = locate(video, event);
    if (!point || !inside(point)) return;

    event.preventDefault();
    const unit = NOTCHES_PER_UNIT[event.deltaMode] ?? NOTCHES_PER_UNIT[0]!;
    sendInput({ type: "wheel", dx: event.deltaX * unit, dy: event.deltaY * unit });
  }

  const onContextMenu = (event: Event) => event.preventDefault();

  video.addEventListener("pointermove", onMove);
  video.addEventListener("pointerdown", onDown);
  video.addEventListener("pointerup", onUp);
  video.addEventListener("pointercancel", onUp);
  video.addEventListener("wheel", onWheel, { passive: false });
  video.addEventListener("contextmenu", onContextMenu);

  return () => {
    window.cancelAnimationFrame(frame);
    window.clearTimeout(fallback);
    video.removeEventListener("pointermove", onMove);
    video.removeEventListener("pointerdown", onDown);
    video.removeEventListener("pointerup", onUp);
    video.removeEventListener("pointercancel", onUp);
    video.removeEventListener("wheel", onWheel);
    video.removeEventListener("contextmenu", onContextMenu);
    // Let go of anything still held, so the sharer is not left with a stuck button.
    for (const held of dragging) {
      const button = BUTTONS[held];
      if (button) sendInput({ type: "mouse-up", button });
    }
    dragging.clear();
  };
}
