<script setup lang="ts">
import { post, state, useAgent } from "../agent.js";
import { t } from "../i18n.js";

useAgent();

/** The app's window can be asked to move itself; a borderless window has no title bar to drag. */
function drag(event: MouseEvent) {
  if (event.button !== 0 || (event.target as Element).closest("button")) return;
  (window as unknown as { ipc?: { postMessage(message: string): void } }).ipc?.postMessage("drag");
}
</script>

<template>
  <div class="indicator" :title="t('Light Assist: drag to move')" @mousedown="drag">
    <span class="status-dot live"></span>
    <span class="indicator-title">{{ state?.session?.unattended ? t("Trusted device connected") : t("Assistant connected") }}</span>
    <span class="chip">{{ t("Screen & sound") }}</span>
    <!-- What the helper can do beyond looking, each with its own way to take it back. -->
    <button
      v-if="state?.session?.access === 'control'"
      class="chip strong"
      type="button"
      :title="t('Take control back')"
      @click="post('/api/session/view')"
    >
      {{ t("Mouse & keyboard") }} <span aria-hidden="true">✕</span>
    </button>
    <button
      v-if="state?.session?.terminal"
      class="chip strong"
      type="button"
      :title="t('Close the terminal')"
      @click="post('/api/session/terminal', { allow: false })"
    >
      {{ t("Terminal") }} <span aria-hidden="true">✕</span>
    </button>
    <span v-if="state?.session?.linkLost" class="chip warn" :title="t('The session ends by itself if your assistant does not come back')">{{ t("Connection lost") }}</span>
    <span v-if="state?.session?.slowConnection" class="chip warn" :title="t('Your assistant sees a simpler picture until the connection improves')">
      {{ t("Slow connection") }}
    </span>
    <span class="spacer"></span>
    <button class="action-button" type="button" :title="t('Open Light Assist')" @click="post('/api/window/show')">{{ t("Open") }}</button>
    <button
      class="action-button danger"
      type="button"
      :title="t('Disconnect your assistant ({key} does the same from anywhere)', { key: state?.stopKey ?? '' })"
      @click="post('/api/request/stop')"
    >
      {{ t("Stop") }}
    </button>
  </div>
</template>
