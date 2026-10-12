import { ref, watch } from "vue";

/** What matters most: text keeps its sharpness, motion its smoothness, response the shortest delay. */
export type Content = "text" | "motion" | "response";

/** The helper's choices. `null` is automatic; a height of 0 is the sharer's screen at its own size. */
export interface QualityChoice {
  content: Content;
  height: number | null;
  fps: number | null;
  bitrateKbps: number | null;
}

/** What the sharer's computer is sending, as it last said. */
export interface QualityTarget {
  width: number;
  height: number;
  fps: number;
  bitrateBps: number;
  content: Content;
}

const SETTING = "light-assist.quality";
const AUTOMATIC: QualityChoice = { content: "text", height: null, fps: null, bitrateKbps: null };

function remembered(): QualityChoice {
  try {
    return { ...AUTOMATIC, ...(JSON.parse(localStorage.getItem(SETTING) ?? "{}") as Partial<QualityChoice>) };
  } catch {
    return { ...AUTOMATIC };
  }
}

/** The helper's choices, remembered by this browser for next time. */
export const qualityChoice = ref<QualityChoice>(remembered());
watch(
  qualityChoice,
  (choice) => {
    try {
      localStorage.setItem(SETTING, JSON.stringify(choice));
    } catch {
      // The choice simply lasts for this visit.
    }
  },
  { deep: true },
);

export const qualityTarget = ref<QualityTarget | null>(null);
/** The sharer's agent finds the connection too slow for a good picture. */
export const connectionSlow = ref(false);

export const HEIGHTS = [0, 2160, 1440, 1080, 720];
export const RATES = [15, 30, 60];
export const BITRATES_KBPS = [2000, 4000, 8000, 15000, 30000];
