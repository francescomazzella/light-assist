<script setup lang="ts">
import { ownTitleBar, windowAction } from "../agent.js";
import { t } from "../i18n.js";

defineProps<{ status: string; tone: "idle" | "waiting" | "online"; footnote: string; settings?: boolean; settingsOpen?: boolean }>();
defineEmits<{ settings: [] }>();

/** The header is the window's title bar where the window has no other: dragging it moves the window. */
function drag(event: MouseEvent) {
  if (ownTitleBar && event.button === 0 && !(event.target as Element).closest("button")) windowAction("drag");
}
</script>

<template>
  <div class="app">
    <header class="app-bar" :class="{ 'title-bar': ownTitleBar }" @mousedown="drag">
      <span class="app-name">
        <span class="mark" aria-hidden="true"><span></span><span></span><span></span><span></span></span>
        Light Assist
      </span>
      <span class="app-status" role="status">
        <span class="status-dot" :class="tone === 'idle' ? '' : tone"></span>
        {{ status }}
        <button
          v-if="settings"
          class="icon-button"
          :class="{ active: settingsOpen }"
          type="button"
          :aria-label="t('Settings')"
          :aria-pressed="settingsOpen"
          :title="t('Settings')"
          @click="$emit('settings')"
        >
          <!-- Three sliders: "options", which a ring of rays (it read as a sun, for themes) did not say. -->
          <svg viewBox="0 0 20 20" aria-hidden="true">
            <path d="M3 5.5h5.2M11.8 5.5H17M3 10h8.2M14.8 10H17M3 14.5h2.2M8.8 14.5H17" />
            <circle cx="10" cy="5.5" r="1.8" />
            <circle cx="13" cy="10" r="1.8" />
            <circle cx="7" cy="14.5" r="1.8" />
          </svg>
        </button>
      </span>
      <!-- The window's own buttons, drawn as Windows draws them, where the window has no title bar of its own. -->
      <span v-if="ownTitleBar" class="caption-buttons">
        <button class="caption-button" type="button" :aria-label="t('Minimize')" :title="t('Minimize')" @click="windowAction('minimize')">
          <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
        </button>
        <button class="caption-button close" type="button" :aria-label="t('Close')" :title="t('Close')" @click="windowAction('close')">
          <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" /></svg>
        </button>
      </span>
    </header>

    <main class="app-body">
      <slot />
    </main>

    <footer class="app-foot">{{ footnote }}</footer>
  </div>
</template>
