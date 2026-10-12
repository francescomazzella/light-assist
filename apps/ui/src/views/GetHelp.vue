<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { Failure, Outcome } from "@light-assist/protocol";
import { newerVersion, openWebsite, post, state } from "../agent.js";
import { Phrase, t } from "../i18n.js";

const deviceName = ref("");
const trusted = ref(false);

/** Makes the connected helper's browser a trusted device, under the name typed for it. */
async function trust() {
  if (await run("/api/session/trust", { name: deviceName.value })) {
    trusted.value = true;
    deviceName.value = "";
  }
}

const failureText = (reason: Failure) =>
  ({
    "invalid-code": t("That code does not match anyone waiting. Ask your assistant to read it again."),
    "bad-request": t("Enter the 6-digit code your assistant sees."),
    "not-asking": t("You are no longer requesting assistance."),
    busy: t("An assistant is already connected."),
    "port-unavailable": t("Light Assist could not open a network port on this computer."),
    "not-active": t("No assistant is connected."),
    version: t("Your assistant's page is a different version."),
    "not-trusted": t("That device is not trusted."),
  })[reason];

const code = ref("");
const busy = ref(false);
const error = ref("");
const copied = ref(false);
const now = ref(Date.now());
let clock: number | undefined;

const canAdmit = computed(() => !busy.value && /^\d{6}$/.test(code.value));
const expiry = computed(() => {
  const seconds = Math.max(0, Math.floor(((state.value?.request?.expiresAt ?? 0) - now.value) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
});
const waiting = computed(() => {
  const count = state.value?.request?.waiting ?? 0;
  if (count === 0) return t("Nobody has opened the address yet.");
  return count === 1 ? t("1 person has the page open and sees a code.") : t("{count} people have the page open. Each sees a different code.", { count });
});

async function run(path: string, body?: unknown) {
  busy.value = true;
  error.value = "";
  const result = await post<Outcome>(path, body);
  busy.value = false;

  if (!result) error.value = t("Light Assist is not reachable.");
  else if (!result.ok) error.value = failureText(result.reason);
  return result?.ok === true;
}

async function admit() {
  if (canAdmit.value && (await run("/api/request/admit", { code: code.value }))) code.value = "";
}

/** The address to give out: the hosted helper page when it can be used, this computer's own otherwise. */
const address = computed(() => state.value?.request?.hostedAddress ?? state.value?.request?.address ?? "");
/** As it is said aloud: a browser adds the rest. */
const spoken = (url: string) => url.replace(/^https?:\/\//, "");

async function copyAddress() {
  try {
    await navigator.clipboard.writeText(address.value);
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1800);
  } catch {
    // The address can still be selected and copied by hand.
  }
}

onMounted(() => (clock = window.setInterval(() => (now.value = Date.now()), 1000)));
onUnmounted(() => window.clearInterval(clock));
</script>

<template>
  <!-- A Mac shows no app its screen until its user says so, in System Settings. -->
  <section v-if="state?.permissions && !state.permissions.screen" class="card attention" aria-labelledby="screen-permission-title">
    <h2 id="screen-permission-title" class="card-title">{{ t("Let Light Assist show your screen") }}</h2>
    <p class="text">
      {{
        t(
          "Your Mac asks for your permission before an app can show your screen to anyone. Turn Light Assist on under Screen & System Audio Recording in System Settings, then quit Light Assist and open it again.",
        )
      }}
    </p>
    <p class="text">{{ t("Until then your assistant can connect, but sees nothing.") }}</p>
    <button class="action-button" type="button" @click="post('/api/permissions', { ask: 'screen' })">{{ t("Open System Settings") }}</button>
  </section>

  <!-- A helper is connected: what they may do, and the way out. -->
  <template v-if="state?.session">
    <section v-if="state.session.controlRequested" class="card attention" role="alertdialog" aria-labelledby="control-title">
      <h2 id="control-title" class="card-title">{{ t("Let your assistant use the mouse and keyboard?") }}</h2>
      <p class="text">
        {{ t("They will be able to click and type on this computer.") }}
        <template v-if="state.session.clipboard">{{ t("Text that either of you copies can then be pasted on the other computer.") }}</template>
        {{ t("You can take control back at any time.") }}
      </p>
      <p v-if="state.permissions && !state.permissions.control" class="text">
        {{ t("Your Mac also asks for your permission: turn Light Assist on under Accessibility in System Settings, or their mouse and keyboard will do nothing.") }}
        <button class="link-button" type="button" @click="post('/api/permissions', { ask: 'control' })">{{ t("Open System Settings") }}</button>
      </p>
      <div class="action-group">
        <button class="action-button primary" type="button" @click="post('/api/session/control', { allow: true })">{{ t("Allow") }}</button>
        <button class="action-button" type="button" @click="post('/api/session/control', { allow: false })">{{ t("Don't allow") }}</button>
      </div>
    </section>

    <section v-if="state.session.terminalRequested" class="card attention" role="alertdialog" aria-labelledby="terminal-title">
      <h2 id="terminal-title" class="card-title">{{ t("Let your assistant open a terminal?") }}</h2>
      <p class="text">{{ t("They will be able to run any command as you, and you will not see what they type. You can close the terminal at any time.") }}</p>
      <div class="action-group">
        <button class="action-button primary" type="button" @click="post('/api/session/terminal', { allow: true })">{{ t("Allow") }}</button>
        <button class="action-button" type="button" @click="post('/api/session/terminal', { allow: false })">{{ t("Don't allow") }}</button>
      </div>
    </section>

    <section class="card" aria-labelledby="session-title">
      <div>
        <h1 id="session-title" class="card-title">{{ state.session.unattended ? t("A trusted device is connected") : t("Your assistant is connected") }}</h1>
        <p class="text">
          <template v-if="state.session.unattended">{{ t("“{device}” connected by itself, as you allow trusted devices to.", { device: state.session.unattended }) }}</template>
          {{
            state.session.hosted
              ? t("From {address}, on a network this computer is on", { address: state.session.helperAddress })
              : t("From {address}", { address: state.session.helperAddress })
          }}
        </p>
      </div>
      <p v-if="state.session.slowConnection" class="slow-connection" role="status">
        {{ t("Slow connection: your assistant sees a simpler picture until it improves.") }}
      </p>
      <ul class="permissions">
        <li>
          <span class="permission-name">{{ t("Screen and sound") }}</span>
          <span v-if="state.permissions && !state.permissions.screen" class="permission-state">{{ t("Not yet allowed in System Settings: they see nothing") }}</span>
          <span v-else-if="state.session.linkLost" class="permission-state">
            {{ t("The connection to your assistant was lost. If it does not come back in half a minute, the session ends.") }}
          </span>
          <span v-else-if="!state.session.linked" class="permission-state">
            {{
              now - state.session.startedAt < 15_000
                ? t("Connecting the two computers…")
                : t("The two computers cannot reach each other. Your assistant has to be on this network, or on your Tailscale network.")
            }}
          </span>
          <span v-else class="permission-state on">{{ t("Seeing your screen, hearing what this computer plays") }}</span>
        </li>
        <li>
          <span class="permission-name">{{ t("Mouse and keyboard") }}</span>
          <span v-if="state.session.access === 'control' && state.permissions && !state.permissions.control" class="permission-state">
            {{ t("Allowed here, but not yet in System Settings") }}
            <button class="link-button" type="button" @click="post('/api/permissions', { ask: 'control' })">{{ t("Open System Settings") }}</button>
          </span>
          <span v-else-if="state.session.access === 'control'" class="permission-state on">
            {{ state.session.clipboard ? t("Controlling this computer, sharing copied text") : t("Controlling this computer") }}
          </span>
          <span v-else class="permission-state">{{ t("Not allowed") }}</span>
          <button v-if="state.session.access === 'control'" class="action-button" type="button" @click="post('/api/session/view')">
            {{ t("Take control back") }}
          </button>
        </li>
        <li>
          <span class="permission-name">{{ t("Terminal") }}</span>
          <span v-if="state.session.terminal" class="permission-state on">{{ t("Open on this computer") }}</span>
          <span v-else class="permission-state">{{ t("Not allowed") }}</span>
          <button v-if="state.session.terminal" class="action-button" type="button" @click="post('/api/session/terminal', { allow: false })">
            {{ t("Close terminal") }}
          </button>
        </li>
      </ul>
    </section>

    <!-- Only with unattended access switched on, and only for a helper on this network. -->
    <section v-if="state.session.canTrust" class="card" aria-labelledby="trust-title">
      <h2 id="trust-title" class="card-title">{{ t("Trust this assistant's browser?") }}</h2>
      <p v-if="trusted" class="text" role="status">{{ t("Done. You can remove it again in the settings.") }}</p>
      <template v-else>
        <p class="text">
          {{
            t(
              "It will be able to connect and use the mouse and keyboard while you are away, without a code and without asking you, from this network or over Tailscale. Do this only for a device of your own or of someone you trust completely.",
            )
          }}
        </p>
        <form class="code-row" @submit.prevent="trust">
          <input v-model="deviceName" class="code-input name-input" maxlength="40" :placeholder="t('A name for it, such as Anna\'s laptop')" :aria-label="t('A name for this device')" />
          <button class="action-button" type="submit" :disabled="busy">{{ t("Trust") }}</button>
        </form>
        <p v-if="error" class="error" role="alert">{{ error }}</p>
      </template>
    </section>

    <div class="spacer"></div>
    <p class="text countdown">
      <Phrase :text="t('To disconnect at once from anywhere, press {key}.')">
        <template #key><kbd>{{ state.stopKey }}</kbd></template>
      </Phrase>
    </p>
    <button class="action-button danger large wide" type="button" @click="post('/api/request/stop')">{{ t("Disconnect") }}</button>
  </template>

  <!-- Asking for help: the address to give out, and where the helper's code goes. -->
  <template v-else-if="state?.request">
    <section class="card" aria-labelledby="address-title">
      <h1 id="address-title" class="card-title">{{ t("1. Ask your assistant to open this in a browser") }}</h1>
      <div class="address-box">
        <span class="help-address">{{ state.request.hostedAddress ? spoken(address) : address }}</span>
        <button class="icon-button" type="button" :aria-label="copied ? t('Address copied') : t('Copy address')" @click="copyAddress">
          <svg v-if="!copied" viewBox="0 0 20 20" aria-hidden="true"><rect x="7" y="7" width="9" height="10" rx="1.5" /><path d="M13 7V4.5A1.5 1.5 0 0 0 11.5 3h-7A1.5 1.5 0 0 0 3 4.5v8A1.5 1.5 0 0 0 4.5 14H7" /></svg>
          <span v-else aria-hidden="true">OK</span>
        </button>
      </div>
      <div v-if="state.request.hostedAddress || state.request.hostedOffline || state.request.fallbackAddress || state.request.tailscaleAddress" class="alternatives">
        <span v-if="state.request.hostedAddress">
          <Phrase :text="t('Without the internet, on this network: {address}')">
            <template #address><code>{{ state.request.address }}</code></template>
          </Phrase>
        </span>
        <span v-else-if="state.request.hostedOffline">
          <Phrase :text="t('Light Assist\'s website cannot be reached from this computer right now, so this address only works on this network. {address} will work again when it can.')">
            <template #address><code>{{ spoken(state.request.hostedOffline) }}</code></template>
          </Phrase>
        </span>
        <span v-if="state.request.fallbackAddress">
          <Phrase :text="state.request.hostedAddress ? t('Or: {address}') : t('If it does not load: {address}')">
            <template #address><code>{{ state.request.fallbackAddress }}</code></template>
          </Phrase>
        </span>
        <span v-if="state.request.tailscaleAddress">
          <Phrase :text="t('Over Tailscale: {address}')">
            <template #address><code>{{ state.request.tailscaleAddress }}</code></template>
          </Phrase>
        </span>
      </div>
    </section>

    <section class="card" aria-labelledby="code-title">
      <h2 id="code-title" class="card-title">{{ t("2. Type the code they see") }}</h2>
      <form class="code-row" @submit.prevent="admit">
        <input
          id="pairing-code"
          v-model="code"
          class="code-input"
          inputmode="numeric"
          maxlength="6"
          autocomplete="off"
          placeholder="000000"
          aria-labelledby="code-title"
        />
        <button class="action-button primary large" type="submit" :disabled="!canAdmit">{{ t("Allow") }}</button>
      </form>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <p v-else class="text">{{ waiting }} {{ t("Allowing lets that person see your screen. Anything more is a separate question.") }}</p>
    </section>

    <div class="spacer"></div>
    <p class="text countdown">{{ t("Closes by itself in {time} if nobody connects.", { time: expiry }) }}</p>
    <button class="action-button neutral large wide" type="button" @click="post('/api/request/stop')">{{ t("Stop the request") }}</button>
  </template>

  <!-- Nothing is happening. -->
  <template v-else>
    <section v-if="state?.ended" class="card attention" role="status">
      <p class="card-title">{{ t("Your last session ended by itself") }}</p>
      <p class="text">
        <template v-if="state.ended === 'connection-lost'">
          {{ t("The connection between the two computers was lost and did not come back. That happens when either of them changes network or goes to sleep.") }}
        </template>
        <template v-else>
          {{
            state.ended === "idle"
              ? t("Your assistant could use this computer but had not done anything for a while.")
              : t("It reached the longest session time you allow.")
          }}
          {{ t("You can change both in the settings.") }}
        </template>
      </p>
    </section>

    <section v-if="state?.unattended?.devices.length" class="card" role="status">
      <p class="card-title">{{ state.unattended.listening ? t("Trusted devices can connect") : t("Trusted devices cannot connect right now") }}</p>
      <p class="text">
        <Phrase
          v-if="state.unattended.listening"
          :text="
            state.unattended.devices.length === 1
              ? t('While Light Assist is running, the device you trust can connect at {address} and use this computer without asking. You can change that in the settings.')
              : t('While Light Assist is running, the {count} devices you trust can connect at {address} and use this computer without asking. You can change that in the settings.', {
                  count: state.unattended.devices.length,
                })
          "
        >
          <template #address
            ><code>{{ spoken(state.unattended.hostedAddress ?? state.unattended.hostedOffline ?? state.unattended.address ?? "") }}</code></template
          >
        </Phrase>
        <template v-else>{{ t("Light Assist could not open a network port on this computer.") }}</template>
      </p>
    </section>

    <section v-if="newerVersion" class="card" role="status">
      <p class="card-title">{{ t("A new version of Light Assist is available") }}</p>
      <p class="text">{{ t("Version {new} is out; this is {current}.", { new: newerVersion, current: state?.version ?? "" }) }}</p>
      <button class="action-button" type="button" @click="openWebsite('download')">{{ t("Get it") }}</button>
    </section>

    <section class="card" aria-labelledby="idle-title">
      <h1 id="idle-title" class="title">{{ t("Request assistance with this computer") }}</h1>
      <p class="text">{{ t("Someone you trust can see your screen from their own browser, with nothing to install.") }}</p>
      <ol class="steps">
        <li>{{ t("You get an address to tell them.") }}</li>
        <li>{{ t("They open it and read you a code, which you type here.") }}</li>
        <li>{{ t("They see your screen. You decide whether they may also use the mouse, keyboard or a terminal.") }}</li>
      </ol>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <button class="action-button primary large wide" type="button" :disabled="busy" @click="run('/api/request/start')">{{ t("Start") }}</button>
    </section>
  </template>
</template>
