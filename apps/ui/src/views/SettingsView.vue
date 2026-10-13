<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import type { AppInfo, Settings } from "@light-assist/protocol";
import { api, checkForUpdates, newerVersion, openWebsite, post, state, updateChecked } from "../agent.js";
import { LANGUAGES, Phrase, day, languageChoice, t } from "../i18n.js";

defineEmits<{ done: [] }>();

/** Minutes, with 0 meaning never. */
const idleChoices = [5, 15, 30, 60, 0];
const maxChoices = [30, 60, 120, 240, 480, 0];

const settings = ref<Settings>();
const failed = ref(false);
const app = ref<AppInfo>();
const appMessage = ref("");
const installing = ref(false);
const mac = computed(() => state.value?.platform === "macos");

function label(minutes: number) {
  if (minutes < 60) return t("{count} minutes", { count: minutes });
  return minutes === 60 ? t("1 hour") : t("{count} hours", { count: minutes / 60 });
}

/** A value saved by hand in the settings file still has to appear in the list. */
const withCurrent = (choices: number[], current: number) => (choices.includes(current) ? choices : [current, ...choices]);

onMounted(async () => {
  try {
    settings.value = (await (await api("/api/settings")).json()) as Settings;
    app.value = (await (await api("/api/app")).json()) as AppInfo;
  } catch {
    failed.value = true;
  }
});

type AppReply = AppInfo | { ok: false; message: string };

async function setStartup(on: boolean) {
  appMessage.value = "";
  const reply = (await (await api("/api/app/startup", { method: "POST", body: JSON.stringify({ on }) })).json()) as AppReply;
  if ("installation" in reply) app.value = reply;
  else appMessage.value = reply.message;
}

/** The installed copy opens its own window; this one closes a moment later. */
async function install() {
  installing.value = true;
  appMessage.value = "";
  const reply = (await (await api("/api/app/install", { method: "POST" })).json()) as { ok: boolean; message?: string };
  if (!reply.ok) {
    installing.value = false;
    appMessage.value = reply.message ?? t("Light Assist could not be installed.");
  }
}

/** Every change is saved at once; there is nothing to confirm. */
async function save() {
  try {
    const response = await api("/api/settings", { method: "POST", body: JSON.stringify(settings.value) });
    settings.value = (await response.json()) as Settings;
    failed.value = false;
  } catch {
    failed.value = true;
  }
}
</script>

<template>
  <section class="card" aria-labelledby="settings-title">
    <h1 id="settings-title" class="card-title">{{ t("Ending sessions automatically") }}</h1>

    <template v-if="settings">
      <label class="setting">
        <span class="setting-name">{{ t("When your assistant does nothing") }}</span>
        <span class="text">{{ t("Applies while they can use the mouse, keyboard or a terminal, so a forgotten session does not stay open.") }}</span>
        <select v-model.number="settings.idleMinutes" class="select" @change="save">
          <option v-for="minutes in withCurrent(idleChoices, settings.idleMinutes)" :key="minutes" :value="minutes">
            {{ minutes === 0 ? t("Never") : t("After {time}", { time: label(minutes) }) }}
          </option>
        </select>
      </label>

      <label class="setting">
        <span class="setting-name">{{ t("Longest session") }}</span>
        <span class="text">{{ t("Any session ends after this long, whatever is happening in it.") }}</span>
        <select v-model.number="settings.maxMinutes" class="select" @change="save">
          <option v-for="minutes in withCurrent(maxChoices, settings.maxMinutes)" :key="minutes" :value="minutes">
            {{ minutes === 0 ? t("No limit") : label(minutes) }}
          </option>
        </select>
      </label>
    </template>

    <p v-if="failed" class="error" role="alert">{{ t("The settings could not be loaded or saved.") }}</p>
  </section>

  <section v-if="settings" class="card" aria-labelledby="clipboard-title">
    <h2 id="clipboard-title" class="card-title">{{ t("Copy and paste") }}</h2>
    <label class="setting check-setting">
      <input v-model="settings.clipboard" type="checkbox" @change="save" />
      <span>
        <span class="setting-name">{{ t("Share copied text with an assistant who has control") }}</span>
        <span class="text">
          {{
            t(
              "While your assistant can use the mouse and keyboard, text copied on either computer can be pasted on the other. Never before you allow control, and only text.",
            )
          }}
        </span>
      </span>
    </label>
  </section>

  <section v-if="settings" class="card" aria-labelledby="unattended-title">
    <h2 id="unattended-title" class="card-title">{{ t("When you are away") }}</h2>
    <label class="setting check-setting">
      <input v-model="settings.unattended" type="checkbox" :disabled="Boolean(state?.request)" @change="save" />
      <span>
        <span class="setting-name">{{ t("Let trusted devices connect by themselves") }}</span>
        <span class="text">
          {{
            t(
              "A device you trust can then see this computer and use its mouse and keyboard without a code and without anyone here to allow it, from this network or a Tailscale network this computer is on. This computer keeps one address, and stays reachable on the network while Light Assist is running and there is a trusted device. It must be switched on, logged in and unlocked.",
            )
          }}
        </span>
        <span class="text">
          {{ t("Use it only on a network you run yourself, such as your home's, or over Tailscale: on a shared network, someone else could get in the same way.") }}
        </span>
        <span v-if="state?.request" class="text">{{ t("Stop the request before changing this.") }}</span>
      </span>
    </label>

    <template v-if="settings.unattended && state?.unattended">
      <p v-if="state.unattended.devices.length === 0" class="text">
        {{ t("No device is trusted yet. To trust one, have it assist you in the usual way, then choose Trust in this window while it is connected.") }}
      </p>
      <ul v-else class="permissions">
        <li v-for="device in state.unattended.devices" :key="device.id">
          <span class="permission-name">{{ device.name }}</span>
          <span class="permission-state">{{ device.lastUsed ? t("Last connected {date}", { date: day(device.lastUsed) }) : t("Has not connected yet") }}</span>
          <button class="action-button" type="button" @click="post('/api/devices/remove', { id: device.id })">{{ t("Remove") }}</button>
        </li>
      </ul>
      <template v-if="state.unattended.address && state.unattended.devices.length">
        <p class="text">
          <Phrase
            :text="
              state.unattended.hostedAddress
                ? t('They connect at {hosted}, or on this network at {address}, in the browser you trusted.')
                : state.unattended.hostedOffline
                  ? t('They connect at {hosted} (which this computer cannot reach right now), or on this network at {address}, in the browser you trusted.')
                  : t('They connect at {address}, in the browser you trusted.')
            "
          >
            <template #hosted
              ><code>{{ state.unattended.hostedAddress ?? state.unattended.hostedOffline }}</code></template
            >
            <template #address
              ><code>{{ state.unattended.address }}</code
              ><template v-if="state.unattended.fallbackAddress"> ({{ state.unattended.fallbackAddress }})</template></template
            >
          </Phrase>
        </p>
        <p v-if="state.unattended.tailscaleAddress" class="text">
          <Phrase :text="t('Over Tailscale the address is {address}, and a browser has to be trusted there separately: it keeps what makes it trusted for one address.')">
            <template #address
              ><code>{{ state.unattended.tailscaleAddress }}</code></template
            >
          </Phrase>
        </p>
      </template>
    </template>
  </section>

  <section v-if="app" class="card" aria-labelledby="computer-title">
    <h2 id="computer-title" class="card-title">{{ t("This computer") }}</h2>

    <label class="setting check-setting">
      <input
        type="checkbox"
        :checked="app.startWithWindows"
        :disabled="app.installation === 'store' && app.startupNote !== null"
        @change="setStartup(($event.target as HTMLInputElement).checked)"
      />
      <span>
        <span class="setting-name">{{ mac ? t("Open at login") : t("Start with Windows") }}</span>
        <span class="text">
          {{
            mac
              ? t("Waits in the menu bar, ready when you need assistance. Nothing is visible on the network until you request it.")
              : t("Waits in the tray, ready when you need assistance. Nothing is visible on the network until you request it.")
          }}
        </span>
        <span v-if="app.startupNote" class="text">{{ app.startupNote }}</span>
      </span>
    </label>

    <p v-if="mac" class="text">
      {{
        app.installation === "installed"
          ? t("In your Applications folder. To remove it, drag it to the Trash.")
          : t("To keep Light Assist, drag it to your Applications folder.")
      }}
    </p>
    <div v-else-if="app.installation === 'portable'" class="setting">
      <span class="setting-name">{{ t("Install Light Assist") }}</span>
      <span class="text">{{ t("Adds it to the Start menu and to Settings → Apps, so you can find it again or remove it. No administrator rights needed.") }}</span>
      <button class="action-button" type="button" :disabled="installing || state?.request !== null" @click="install">
        {{ installing ? t("Installing…") : t("Install on this computer") }}
      </button>
      <span v-if="state?.request" class="text">{{ t("Stop the request first.") }}</span>
    </div>
    <p v-else class="text">
      {{ app.installation === "store" ? t("Installed from the Microsoft Store, which keeps it up to date.") : t("Installed. Remove it from Settings → Apps.") }}
    </p>

    <label class="setting">
      <span class="setting-name">{{ t("Language") }}</span>
      <select v-model="languageChoice" class="select">
        <option :value="null">{{ t("The same as this computer") }}</option>
        <option v-for="(name, code) in LANGUAGES" :key="code" :value="code">{{ name }}</option>
      </select>
    </label>

    <p v-if="appMessage" class="error" role="alert">{{ appMessage }}</p>
  </section>

  <section class="card" aria-labelledby="about-title">
    <h2 id="about-title" class="card-title">{{ t("About Light Assist") }}</h2>
    <dl class="about">
      <dt>{{ t("Version") }}</dt>
      <dd>{{ state?.version }}</dd>
      <dt>{{ t("Updates") }}</dt>
      <dd>
        <template v-if="state?.installation === 'store'">{{ t("Brought by the Microsoft Store") }}</template>
        <template v-else-if="!state?.website">{{ t("Not checked: this copy was built without a website") }}</template>
        <template v-else-if="newerVersion">
          {{ t("Version {new} is available", { new: newerVersion }) }}
          <button class="link-button" type="button" @click="openWebsite('download')">{{ t("Get it") }}</button>
        </template>
        <template v-else-if="updateChecked?.ok">{{ t("Up to date") }}</template>
        <template v-else-if="updateChecked">
          {{ t("The website could not be reached") }}
          <button class="link-button" type="button" @click="checkForUpdates">{{ t("Try again") }}</button>
        </template>
        <template v-else>{{ t("Checking…") }}</template>
      </dd>
      <dt>{{ t("License") }}</dt>
      <dd>
        {{ t("Free software under the GNU AGPL 3.0") }}
        <button class="link-button" type="button" @click="openWebsite('source')">{{ t("Source code") }}</button>
      </dd>
      <template v-if="state?.website">
        <dt>{{ t("Privacy") }}</dt>
        <dd>
          <button class="link-button" type="button" @click="openWebsite('privacy')">{{ t("Privacy policy") }}</button> ·
          <button class="link-button" type="button" @click="openWebsite('terms')">{{ t("Terms of use") }}</button>
        </dd>
      </template>
    </dl>
    <p class="text">© 2026 Francesco Mazzella</p>
  </section>

  <div class="spacer"></div>
  <!-- Always within reach, however far down the page is read. -->
  <div class="sticky-actions">
    <button class="action-button neutral large wide" type="button" @click="$emit('done')">{{ t("Done") }}</button>
  </div>
</template>
