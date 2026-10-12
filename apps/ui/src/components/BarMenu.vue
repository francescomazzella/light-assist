<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";

defineProps<{ label: string }>();
/** Whether the menu is showing; the bar it sits in stays up for as long as it is. */
const open = defineModel<boolean>("open", { default: false });

const root = ref<HTMLElement>();

function closeIfOutside(event: Event) {
  if (open.value && !root.value?.contains(event.target as Node)) open.value = false;
}

onMounted(() => window.addEventListener("pointerdown", closeIfOutside, true));
onUnmounted(() => window.removeEventListener("pointerdown", closeIfOutside, true));
</script>

<template>
  <div ref="root" class="bar-menu">
    <button class="action-button" type="button" aria-haspopup="menu" :aria-expanded="open" @click="open = !open">
      {{ label }} <span class="caret" aria-hidden="true">▾</span>
    </button>
    <!-- Hidden, not removed: what is in it (the connection figures, for one) stays up to date. -->
    <div v-show="open" class="bar-menu-items" role="menu" @click="open = false">
      <slot />
    </div>
  </div>
</template>
