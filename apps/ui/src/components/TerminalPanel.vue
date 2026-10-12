<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import "@xterm/xterm/css/xterm.css";
import { t } from "../i18n.js";
import { sendTerminal, terminalSink } from "../link.js";

defineEmits<{ hide: [] }>();

const mount = ref<HTMLDivElement>();
const closed = ref(false);
let dispose = () => {};
let start = () => {};

onMounted(async () => {
  // xterm.js is the largest thing on the page, so it is only fetched when a terminal is actually opened.
  const [{ Terminal }, { FitAddon }] = await Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit")]);
  if (!mount.value) return;

  const terminal = new Terminal({
    cursorBlink: true,
    fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, ui-monospace, monospace',
    fontSize: 13,
    theme: { background: "#0c0e0c", foreground: "#e6eae4" },
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(mount.value);
  fit.fit();

  terminal.onData((data) => sendTerminal({ type: "stdin", data }));
  terminal.onResize(({ cols, rows }) => sendTerminal({ type: "resize", cols, rows }));
  terminalSink.output = (bytes) => terminal.write(bytes);
  terminalSink.closed = () => (closed.value = true);

  // The first size the agent hears is what starts the shell.
  start = () => {
    closed.value = false;
    terminal.reset();
    sendTerminal({ type: "resize", cols: terminal.cols, rows: terminal.rows });
    terminal.focus();
  };
  start();

  const resized = new ResizeObserver(() => fit.fit());
  resized.observe(mount.value);
  dispose = () => {
    resized.disconnect();
    terminalSink.output = undefined;
    terminalSink.closed = undefined;
    terminal.dispose();
  };
});

onUnmounted(() => dispose());
</script>

<template>
  <section class="terminal-panel" :aria-label="t('Terminal on their computer')">
    <header class="terminal-bar">
      <span>{{ t("Terminal on their computer") }}</span>
      <button class="action-button" type="button" @click="$emit('hide')">{{ t("Hide") }}</button>
    </header>
    <div ref="mount" class="terminal-mount"></div>
    <div v-if="closed" class="terminal-closed" role="status">
      <p>{{ t("The terminal was closed.") }}</p>
      <button class="action-button" type="button" @click="start()">{{ t("Start a new one") }}</button>
    </div>
  </section>
</template>
