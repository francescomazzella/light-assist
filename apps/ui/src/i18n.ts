import { defineComponent, ref, watch, type VNodeChild } from "vue";
import { it } from "./locales/it.js";

/**
 * The app in the reader's own language.
 *
 * Everything the app says is written in English where it is said, and looked up here on its way to
 * the screen: `t("Allow")`. The English text is the key, so the code reads as the app does, and a
 * language is one file that gives each English text its own (`locales/it.ts`). What a language does
 * not have is shown in English. `scripts/check-i18n.mjs` says when a language has fallen behind.
 *
 * Each person sees their own language: the sharer by their computer's, the helper by their browser's.
 */

/** A language's texts, by the English they stand for. */
export type Catalog = Record<string, string>;

const catalogs: Record<string, Catalog> = { it };

/** The languages there are, by their own names, for someone choosing one. */
export const LANGUAGES: Record<string, string> = { en: "English", it: "Italiano" };

const SETTING = "light-assist.language";

function chosen(): string | null {
  try {
    const choice = localStorage.getItem(SETTING);
    return choice && choice in LANGUAGES ? choice : null;
  } catch {
    return null;
  }
}

/** The first of the reader's languages that the app has, or English. */
function preferred() {
  for (const wanted of navigator.languages ?? [navigator.language]) {
    const language = wanted.toLowerCase().split("-")[0] ?? "";
    if (language in LANGUAGES) return language;
  }
  return "en";
}

/** The reader's own choice, or `null` to follow their system or browser. */
export const languageChoice = ref<string | null>(chosen());
/** The language everything is shown in. */
export const language = ref(languageChoice.value ?? preferred());

watch(languageChoice, (choice) => {
  language.value = choice ?? preferred();
  try {
    if (choice) localStorage.setItem(SETTING, choice);
    else localStorage.removeItem(SETTING);
  } catch {
    // The choice simply lasts for this visit.
  }
});
watch(language, (now) => (document.documentElement.lang = now), { immediate: true });

/**
 * `text` in the reader's language, with each `{name}` in it replaced by that value.
 * Reads the language as it goes, so whatever shows it is drawn again when the language changes.
 */
export function t(text: string, values?: Record<string, string | number>): string {
  const translated = catalogs[language.value]?.[text] ?? text;
  return values ? translated.replace(/\{(\w+)\}/g, (whole, name: string) => String(values[name] ?? whole)) : translated;
}

/**
 * A sentence with something other than text in the middle of it: a key, an address, a button.
 * The sentence is written whole, with `{name}` where each one goes, so that a language can put it
 * where its own grammar wants it. Each `{name}` is filled by the slot of that name.
 */
export const Phrase = defineComponent({
  props: { text: { type: String, required: true } },
  setup(props, { slots }) {
    return () =>
      props.text.split(/(\{\w+\})/).map((piece): VNodeChild => {
        const name = /^\{(\w+)\}$/.exec(piece)?.[1];
        return name ? (slots[name]?.() ?? piece) : piece;
      });
  },
});

/** A date as the reader's language writes it. */
export function day(time: number) {
  return new Date(time).toLocaleDateString(language.value);
}

