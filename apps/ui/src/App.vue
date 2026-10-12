<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Shell from "./components/Shell.vue";
import GetHelp from "./views/GetHelp.vue";
import SettingsView from "./views/SettingsView.vue";
import { checkForUpdates, online, state, useAgent } from "./agent.js";
import { t } from "./i18n.js";

useAgent();
// Once the agent has said where the website is.
watch(
  () => state.value?.website,
  (website) => website && void checkForUpdates(),
);

const showingSettings = ref(false);

const status = computed(() => {
  if (!online.value) return t("Starting…");
  if (state.value?.session) return t("Assistant connected");
  if (state.value?.request) return t("Waiting for your assistant");
  return t("Not sharing");
});
const tone = computed(() => (state.value?.session ? "online" : state.value?.request ? "waiting" : "idle"));
const footnote = computed(() =>
  state.value?.request
    ? t("Stopping makes this computer invisible on the network again.")
    : t("Nothing on this computer is visible on the network until you request assistance."),
);
</script>

<template>
  <Shell :status="status" :tone="tone" :footnote="footnote" settings :settings-open="showingSettings" @settings="showingSettings = !showingSettings">
    <SettingsView v-if="showingSettings" @done="showingSettings = false" />
    <GetHelp v-else />
  </Shell>
</template>
