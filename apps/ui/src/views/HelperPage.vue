<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import BarMenu from "../components/BarMenu.vue";
import Shell from "../components/Shell.vue";
import TerminalPanel from "../components/TerminalPanel.vue";
import {
  access,
  code,
  connect,
  control,
  ended,
  leave,
  phase,
  requestControl,
  requestTerminal,
  retryLink,
  terminal,
} from "../helper-session.js";
import { clipboardNotice, copyWaiting } from "../clipboard.js";
import { forgetDevice, justTrusted, trustedAs } from "../device.js";
import { downloads, roomNamed } from "../hosted.js";
import { Phrase, language, t } from "../i18n.js";
import { blocked, currentScreen, encoderName, linkError, linkLatency, linkPhase, remoteCursor, remoteSound, remoteVideo, screenUnavailable, screens, selectScreen, sharerPlatform, videoStats, yielding } from "../link.js";
import { BITRATES_KBPS, HEIGHTS, RATES, connectionSlow, qualityChoice, qualityTarget } from "../quality.js";
import { SPECIAL_KEYS, commandAsControl, controlAsCommand, forwardKeyboard, forwardMouse, hasCommandKey, pressKeys } from "../remote-input.js";

/** On the hosted page opened by itself: the words of the help address, as the helper types them. */
const words = ref("");
function openRoom() {
  const name = roomNamed(words.value);
  if (name) location.assign(`/${name}`);
}

const BAR_IDLE_MS = 3000;
/** While controlling, the bar only comes back when the pointer touches the top edge. */
const BAR_EDGE_PX = 4;

const screen = ref<HTMLVideoElement>();

const fullscreen = ref(false);
const barAwake = ref(true);
const barHovered = ref(false);
let barTimer: number | undefined;

const terminalShown = ref(false);
const terminalWaiting = computed(() => terminal.value === "requested" || terminal.value === "denied");
const keysOpen = ref(false);
const moreOpen = ref(false);
const qualityOpen = ref(false);

function heightName(height: number) {
  return height === 0 ? t("Their screen's size") : height === 2160 ? "4K (2160p)" : `${height}p`;
}
function megabits(bps: number) {
  return `${(bps / 1_000_000).toFixed(1)} Mbps`;
}

const SOUND_SETTING = "light-assist.sound";
const speaker = ref<HTMLAudioElement>();
function rememberedSound() {
  try {
    return localStorage.getItem(SOUND_SETTING) !== "off";
  } catch {
    return true;
  }
}
/** Whether the helper wants to hear the sharer's computer. On unless turned off. */
const soundOn = ref(rememberedSound());
/** Browsers play no sound on a page nobody has clicked or typed on yet. */
const soundWaiting = ref(false);

async function playSound() {
  const audio = speaker.value;
  if (!audio) return;
  audio.muted = !soundOn.value;
  if (!soundOn.value) return;
  try {
    await audio.play();
    soundWaiting.value = false;
  } catch {
    soundWaiting.value = true;
  }
}
watch([soundOn, speaker, remoteSound], playSound, { flush: "post" });
watch(soundOn, (on) => {
  try {
    localStorage.setItem(SOUND_SETTING, on ? "on" : "off");
  } catch {
    // The choice simply lasts for this visit.
  }
});
/** The first click or key press anywhere on the page is what the browser was waiting for. */
function soundAllowed() {
  if (soundWaiting.value) void playSound();
}

/**
 * The moments a browser lets a page copy something: while the helper is clicking or typing on it.
 * Browsers differ in which of these count, so text waiting to be copied is offered at each.
 */
const COPY_GESTURES = ["pointerdown", "mousedown", "pointerup", "click", "keydown", "keyup"] as const;

const STREAM_INFO_SETTING = "light-assist.stream-info";
function rememberedStreamInfo() {
  try {
    return localStorage.getItem(STREAM_INFO_SETTING) === "on";
  } catch {
    return false;
  }
}
/** Whether the connection's figures are shown in the corner of the screen. Off unless asked for. */
const streamInfoShown = ref(rememberedStreamInfo());
watch(streamInfoShown, (on) => {
  try {
    localStorage.setItem(STREAM_INFO_SETTING, on ? "on" : "off");
  } catch {
    // The choice simply lasts for this visit.
  }
});

// The bar stays up while it has something to say or a menu is open; otherwise it gets out of the way of the screen.
const barVisible = computed(
  () =>
    barAwake.value ||
    barHovered.value ||
    keysOpen.value ||
    moreOpen.value ||
    qualityOpen.value ||
    !remoteVideo.value ||
    screenUnavailable.value ||
    control.value !== "idle" ||
    terminalWaiting.value,
);

function toggleTerminal() {
  if (terminal.value === "granted") terminalShown.value = !terminalShown.value;
  else if (terminal.value !== "requested") requestTerminal();
}

// The panel opens when the sharer says yes, and goes away when they close the terminal.
watch(terminal, (now) => (terminalShown.value = now === "granted"));

function onPointerMove(event: MouseEvent) {
  // Every movement belongs to the sharer's screen while controlling, so the bar must not keep covering it.
  if (access.value !== "control" || event.clientY <= BAR_EDGE_PX) wake();
}

function wake() {
  barAwake.value = true;
  window.clearTimeout(barTimer);
  barTimer = window.setTimeout(() => (barAwake.value = false), BAR_IDLE_MS);
}

async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) await document.exitFullscreen();
    else await document.documentElement.requestFullscreen();
  } catch {
    // The browser refused; the page simply stays as it is.
  }
}

/**
 * In full screen, on a secure page, some browsers will hand over the keys they otherwise keep: the
 * Windows key, Alt+Tab, Esc. They then reach the sharer's computer like any other key. (Leaving full
 * screen is then done by holding Esc, which the browser says when it starts.)
 */
const keyboard = (navigator as Navigator & { keyboard?: { lock(): Promise<void>; unlock(): void } }).keyboard;

function onFullscreenChange() {
  fullscreen.value = document.fullscreenElement !== null;
  if (fullscreen.value) void keyboard?.lock().catch(() => undefined);
  else keyboard?.unlock();
}

// Fullscreen belongs to the session: leave it when the session ends.
watch(phase, (now) => {
  if (now === "admitted") wake();
  else if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
});

// The mouse and keyboard are forwarded exactly while the sharer allows control and there is a picture to act on.
watch(
  [screen, access],
  ([video, level], _previous, onCleanup) => {
    if (!video || level !== "control") return;
    const stopMouse = forwardMouse(video);
    const stopKeyboard = forwardKeyboard();
    onCleanup(() => {
      stopMouse();
      stopKeyboard();
    });
  },
  { flush: "post" },
);

/**
 * The one modifier the two keyboards do not share, when they do not: this helper's Command for a
 * sharer on Windows, or this helper's Ctrl for a sharer on a Mac.
 */
const modifierChoice = computed(() => {
  if (hasCommandKey && sharerPlatform.value === "windows") {
    return { on: commandAsControl, label: t("⌘ acts as Ctrl"), title: t("When on, Command with C, V, X, A, Z, Y, S or F acts as Ctrl on their computer. Otherwise Command is their Windows key.") };
  }
  if (!hasCommandKey && sharerPlatform.value === "macos") {
    return { on: controlAsCommand, label: t("Ctrl acts as ⌘"), title: t("When on, Ctrl with C, V, X, A, Z, Y, S or F acts as Command on their Mac. Otherwise Ctrl is their Control key.") };
  }
  return null;
});

function toggleModifier(event: MouseEvent) {
  const choice = modifierChoice.value;
  if (choice) choice.on.value = !choice.on.value;
  (event.currentTarget as HTMLElement).blur();
}

function pressSpecial(codes: readonly string[], event: MouseEvent) {
  pressKeys(codes);
  // A focused button would be "clicked" again by the next Enter or Space meant for the sharer.
  (event.currentTarget as HTMLElement).blur();
}

// Ctrl+W and similar shortcuts cannot be kept from the browser, so at least ask before the page goes away.
function confirmLeaving(event: BeforeUnloadEvent) {
  if (phase.value === "admitted") event.preventDefault();
}

onMounted(() => {
  document.addEventListener("fullscreenchange", onFullscreenChange);
  window.addEventListener("beforeunload", confirmLeaving);
  window.addEventListener("pointerdown", soundAllowed, true);
  window.addEventListener("keydown", soundAllowed, true);
  for (const gesture of COPY_GESTURES) window.addEventListener(gesture, copyWaiting, true);
});
onUnmounted(() => {
  document.removeEventListener("fullscreenchange", onFullscreenChange);
  window.removeEventListener("beforeunload", confirmLeaving);
  window.removeEventListener("pointerdown", soundAllowed, true);
  window.removeEventListener("keydown", soundAllowed, true);
  for (const gesture of COPY_GESTURES) window.removeEventListener(gesture, copyWaiting, true);
  window.clearTimeout(barTimer);
});
</script>

<template>
  <main v-if="phase === 'admitted'" class="viewer" @mousemove="onPointerMove" @touchstart.passive="wake">
    <audio v-if="remoteSound" ref="speaker" :srcObject.prop="remoteSound" autoplay></audio>
    <video v-if="remoteVideo" ref="screen" class="viewer-screen" :style="{ cursor: access === 'control' && !yielding ? remoteCursor : undefined }" :srcObject.prop="remoteVideo" autoplay muted playsinline></video>
    <div v-if="!remoteVideo || linkPhase !== 'open' || screenUnavailable" class="viewer-wait" role="status">
      <template v-if="linkPhase === 'failed'">
        <p>{{ linkError }}</p>
        <button class="action-button" type="button" @click="retryLink">{{ t("Retry") }}</button>
      </template>
      <p v-else-if="screenUnavailable && blocked === 'screen-not-permitted'">{{ t("Their Mac has not allowed Light Assist to show its screen yet.") }}</p>
      <p v-else-if="screenUnavailable">{{ t("Their screen cannot be shown right now. It may be locked or turned off.") }}</p>
      <p v-else>{{ t("Opening a direct connection…") }}</p>
    </div>

    <div class="viewer-bar" :class="{ hidden: !barVisible }" @mouseenter="barHovered = true" @mouseleave="barHovered = false">
      <span class="status-dot" :class="{ online: linkPhase === 'open' }"></span>
      <span class="viewer-access">
        <template v-if="control === 'requested'">{{ t("Waiting for them to allow control…") }}</template>
        <template v-else-if="control === 'denied'">{{ t("They did not allow control") }}</template>
        <template v-else-if="terminal === 'requested'">{{ t("Waiting for them to allow the terminal…") }}</template>
        <template v-else-if="terminal === 'denied'">{{ t("They did not allow the terminal") }}</template>
        <template v-else>{{ access === "control" ? t("Controlling their computer") : t("Viewing their screen") }}</template>
      </span>
      <!-- The one thing to do next sits in the bar; everything occasional is in a menu. -->
      <div class="action-group">
        <button v-if="access === 'view'" class="action-button primary" type="button" :disabled="control === 'requested'" @click="requestControl">
          {{ t("Request control") }}
        </button>
        <BarMenu v-else v-model:open="keysOpen" :label="t('Keys')">
          <p class="menu-heading">{{ t("Press on their computer") }}</p>
          <button v-for="(codes, label) in SPECIAL_KEYS[sharerPlatform]" :key="label" class="menu-item" type="button" role="menuitem" @click="pressSpecial(codes, $event)">
            <kbd>{{ label }}</kbd>
          </button>
          <!-- Only when one keyboard has a Command key and the other does not. -->
          <button
            v-if="modifierChoice"
            class="menu-item divided"
            type="button"
            role="menuitemcheckbox"
            :aria-checked="modifierChoice.on.value"
            :title="modifierChoice.title"
            @click="toggleModifier"
          >
            <span class="menu-check" aria-hidden="true">{{ modifierChoice.on.value ? "✓" : "" }}</span>
            {{ modifierChoice.label }}
          </button>
        </BarMenu>

        <BarMenu v-model:open="qualityOpen" :label="t('Quality')">
          <p class="menu-heading">{{ t("Prefer") }}</p>
          <button
            v-for="[content, name] in ([['text', t('Sharp text')], ['motion', t('Smooth motion')], ['response', t('Fast response')]] as const)"
            :key="content"
            class="menu-item"
            type="button"
            role="menuitemradio"
            :aria-checked="qualityChoice.content === content"
            @click="qualityChoice.content = content"
          >
            <span class="menu-check" aria-hidden="true">{{ qualityChoice.content === content ? "✓" : "" }}</span>
            {{ name }}
          </button>
          <hr class="menu-rule" />
          <!-- Picking from a list must not close the menu underneath it. -->
          <label class="menu-field" @click.stop>
            {{ t("Resolution") }}
            <select v-model="qualityChoice.height">
              <option :value="null">{{ t("Auto") }}</option>
              <option v-for="height in HEIGHTS" :key="height" :value="height">{{ heightName(height) }}</option>
            </select>
          </label>
          <label class="menu-field" @click.stop>
            {{ t("Frame rate") }}
            <select v-model="qualityChoice.fps">
              <option :value="null">{{ t("Auto") }}</option>
              <option v-for="rate in RATES" :key="rate" :value="rate">{{ rate }} FPS</option>
            </select>
          </label>
          <label class="menu-field" @click.stop>
            {{ t("Bitrate") }}
            <select v-model="qualityChoice.bitrateKbps">
              <option :value="null">{{ t("Auto") }}</option>
              <option v-for="kbps in BITRATES_KBPS" :key="kbps" :value="kbps">{{ kbps / 1000 }} Mbps</option>
            </select>
          </label>
          <p v-if="qualityTarget" class="menu-heading menu-now">
            {{ t("Now {size} · {fps} FPS · {bitrate}", { size: `${qualityTarget.width}×${qualityTarget.height}`, fps: qualityTarget.fps, bitrate: megabits(qualityTarget.bitrateBps) }) }}
          </p>
        </BarMenu>

        <BarMenu v-model:open="moreOpen" :label="t('More')">
          <!-- Only when there is a choice to make. -->
          <template v-if="screens.length > 1">
            <p class="menu-heading">{{ t("Their screen") }}</p>
            <button
              v-for="(each, index) in screens"
              :key="each.id"
              class="menu-item"
              type="button"
              role="menuitemradio"
              :aria-checked="each.id === currentScreen"
              @click="selectScreen(each.id)"
            >
              <span class="menu-check" aria-hidden="true">{{ each.id === currentScreen ? "✓" : "" }}</span>
              {{ t("Screen {number}", { number: index + 1 }) }}<span class="menu-detail">{{ each.width }}×{{ each.height }}{{ each.primary ? ` · ${t("main")}` : "" }}</span>
            </button>
            <hr class="menu-rule" />
          </template>
          <button class="menu-item" type="button" role="menuitem" :disabled="terminal === 'requested'" @click="toggleTerminal">
            {{ terminal === "granted" ? (terminalShown ? t("Hide terminal") : t("Show terminal")) : t("Ask for a terminal") }}
          </button>
          <!-- A trusted device can give that up from its own side. -->
          <button
            v-if="trustedAs !== null"
            class="menu-item"
            type="button"
            role="menuitem"
            :title="t('This browser can connect to their computer while nobody is there. Forgetting it means asking them for a code again.')"
            @click="forgetDevice"
          >
            {{ t("Stop being a trusted device") }}
          </button>
          <button class="menu-item" type="button" role="menuitemcheckbox" :aria-checked="streamInfoShown" @click="streamInfoShown = !streamInfoShown">
            <span class="menu-check" aria-hidden="true">{{ streamInfoShown ? "✓" : "" }}</span>
            {{ t("Show stream info") }}
          </button>
        </BarMenu>

        <button
          v-if="remoteSound"
          class="action-button icon"
          :class="{ waiting: soundOn && soundWaiting }"
          type="button"
          :title="!soundOn ? t('Hear their computer') : soundWaiting ? t('Click to hear their computer') : t('Mute their computer')"
          :aria-label="soundOn ? t('Mute their computer') : t('Hear their computer')"
          :aria-pressed="soundOn"
          @click="soundOn = soundWaiting ? true : !soundOn"
        >
          <svg viewBox="0 0 20 20" aria-hidden="true">
            <path d="M3 8v4h3l4 3V5L6 8H3z" />
            <path v-if="soundOn && !soundWaiting" d="M13 7.5a3.5 3.5 0 0 1 0 5M15 5a7 7 0 0 1 0 10" />
            <path v-else d="M13 8l4 4M17 8l-4 4" />
          </svg>
        </button>
        <button
          class="action-button icon"
          type="button"
          :title="fullscreen ? t('Exit full screen') : t('Full screen')"
          :aria-label="fullscreen ? t('Exit full screen') : t('Full screen')"
          @click="toggleFullscreen"
        >
          <svg viewBox="0 0 20 20" aria-hidden="true">
            <path v-if="fullscreen" d="M8 4v4H4M16 8h-4V4M12 16v-4h4M4 12h4v4" />
            <path v-else d="M4 8V4h4M12 4h4v4M16 12v4h-4M8 16H4v-4" />
          </svg>
        </button>
        <button class="action-button" type="button" @click="leave">{{ t("Disconnect") }}</button>
      </div>
    </div>

    <!-- The connection's figures, for whoever asks to see them. Never in the way of the pointer. -->
    <p v-show="streamInfoShown && (videoStats || linkLatency !== null)" class="viewer-stats stream-info">
      <template v-if="videoStats && videoStats.width > 0">
        {{ videoStats.width }}×{{ videoStats.height }} · {{ videoStats.fps }} FPS<template v-if="linkLatency !== null"> · {{ t("link {ms} ms", { ms: linkLatency }) }}</template>
        · {{ t("buffered {ms} ms", { ms: videoStats.bufferMs }) }}<template v-if="encoderName"> · {{ encoderName }}</template>
        <template v-if="qualityTarget"> · {{ t("sent {size} at {bitrate}", { size: `${qualityTarget.width}×${qualityTarget.height}`, bitrate: megabits(qualityTarget.bitrateBps) }) }}</template>
      </template>
      <template v-else-if="linkLatency !== null">{{ t("link {ms} ms", { ms: linkLatency }) }}</template>
    </p>

    <p v-if="connectionSlow" class="viewer-notice slow" role="status">{{ t("The connection is slow, so the picture is simpler than usual.") }}</p>

    <button v-if="justTrusted" class="viewer-notice trusted action" type="button" @click="justTrusted = false">
      {{ t("They now trust this browser: it can connect to this computer by itself, at this same address. Click to dismiss.") }}
    </button>

    <!-- Any click or key press on the page copies it; the button is for a helper who is only watching. -->
    <button v-if="clipboardNotice === 'waiting'" class="viewer-notice clipboard action" type="button">
      {{ t("They copied some text. Click here to copy it to this computer.") }}
    </button>
    <p v-else-if="clipboardNotice" class="viewer-notice clipboard" role="status">
      {{
        clipboardNotice === "copied"
          ? t("Copied from their computer")
          : clipboardNotice === "too-large"
            ? t("What was copied on their computer is too much text to bring over.")
            : t("That is too much text to paste on their computer.")
      }}
    </p>

    <p v-if="blocked === 'screen-not-permitted'" class="viewer-notice" role="status">
      {{ t("Their Mac has not been told to let Light Assist show its screen yet. They have to allow it in System Settings, then open Light Assist again.") }}
    </p>
    <p v-else-if="blocked === 'protected-screen' && sharerPlatform === 'macos'" class="viewer-notice" role="status">
      {{ t("Their Mac is locked. You can't see or use it until they unlock it themselves.") }}
    </p>
    <p v-else-if="blocked === 'protected-screen'" class="viewer-notice" role="status">
      {{ t("Windows is showing a permission prompt or the lock screen on their computer. You can't see or use it: they have to answer it themselves.") }}
    </p>
    <p v-else-if="blocked === 'control-not-permitted' && access === 'control'" class="viewer-notice" role="status">
      {{ t("Their Mac has not been told to let Light Assist use its mouse and keyboard yet, so yours do nothing. They can allow it in System Settings.") }}
    </p>
    <p v-else-if="blocked === 'elevated-window' && access === 'control'" class="viewer-notice" role="status">
      {{ t("The window in front on their computer runs as administrator, so your mouse and keyboard don't work in it.") }}
    </p>
    <p v-else-if="yielding && access === 'control'" class="viewer-notice" role="status">
      {{ t("They are using their mouse or keyboard. Yours will work again when they stop.") }}
    </p>

    <!-- Kept alive while hidden, so hiding the panel does not end the shell. -->
    <TerminalPanel v-if="terminal === 'granted'" v-show="terminalShown" @hide="terminalShown = false" />
  </main>

  <Shell
    v-else
    :status="phase === 'code' ? t('Waiting to be let in') : phase === 'connecting' ? t('Connecting…') : phase === 'which' ? t('Assist someone') : t('Not connected')"
    :tone="phase === 'code' ? 'waiting' : 'idle'"
    :footnote="t('You are assisting the person at this address. They decide what you can see and do.')"
  >
    <section v-if="phase === 'code'" class="card" aria-labelledby="code-title">
      <h1 id="code-title" class="card-title">{{ t("Read this code to the person you are assisting") }}</h1>
      <span class="pairing-code" :aria-label="t('Pairing code')">{{ code }}</span>
      <p class="text">{{ t("They type it into Light Assist and choose Allow. Keep this page open; the code is only for you and changes every few minutes.") }}</p>
    </section>

    <section v-else-if="phase === 'which'" class="card" aria-labelledby="which-title">
      <h1 id="which-title" class="card-title">{{ t("Who are you assisting?") }}</h1>
      <p class="text">
        <Phrase :text="t('Light Assist on their computer shows an address that ends in a few words, such as {example}. Type those words here.')">
          <template #example><code>maple-river</code></template>
        </Phrase>
      </p>
      <form class="code-row" @submit.prevent="openRoom">
        <input v-model="words" class="code-input name-input" autocomplete="off" autocapitalize="none" spellcheck="false" placeholder="maple-river" aria-labelledby="which-title" />
        <button class="action-button primary large" type="submit" :disabled="!roomNamed(words)">{{ t("Open") }}</button>
      </form>
    </section>

    <section v-else-if="phase === 'connecting'" class="card">
      <p class="text" role="status">{{ t("Connecting…") }}</p>
    </section>

    <section v-else class="card">
      <p class="card-title" role="status">{{ ended }}</p>
      <button class="action-button primary large wide" type="button" @click="connect">{{ t("Try again") }}</button>
    </section>

    <!-- The website's front door is also where someone who wants assistance finds the app. -->
    <section v-if="phase === 'which'" class="card" aria-labelledby="get-title">
      <h2 id="get-title" class="card-title">{{ t("Need assistance yourself?") }}</h2>
      <p class="text">{{ t("Install Light Assist on the computer that needs it. Whoever assists you needs only a browser.") }}</p>
      <div class="downloads">
        <a v-for="download in downloads()" :key="download.name" class="action-button" :href="download.url" target="_blank" rel="noopener">
          {{ download.name }}<span class="download-note">{{ download.note }}</span>
        </a>
      </div>
      <p class="text">
        <a class="link-button" :href="language === 'en' ? '/privacy' : `/privacy-${language}`">{{ t("Privacy") }}</a> ·
        <a class="link-button" :href="language === 'en' ? '/terms' : `/terms-${language}`">{{ t("Terms of use") }}</a>
      </p>
    </section>
  </Shell>
</template>
