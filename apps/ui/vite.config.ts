import { readFileSync, renameSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import vue from "@vitejs/plugin-vue";
import { defineConfig, loadEnv } from "vite";

const uiRoot = fileURLToPath(new URL(".", import.meta.url));

// The app's version, from the one place it is set, and this build's own mark: the hosted page compares
// its mark with the website's to know that a newer page has been published.
const version = /^version = "(.+)"/m.exec(readFileSync(`${uiRoot}../agent-rs/Cargo.toml`, "utf8"))?.[1] ?? "0.0.0";
const build = new Date().toISOString();
process.env.VITE_APP_VERSION = version;
process.env.VITE_BUILD = build;

// Built twice. By default: the three pages the app carries inside it. With `--mode hosted`: the
// helper page alone, for a website, where it reaches sharers through the relay named by
// VITE_RELAY_URL (see `src/hosted.ts`).
export default defineConfig(({ mode }) => {
  const hosted = mode === "hosted";
  if (hosted && !loadEnv(mode, uiRoot).VITE_RELAY_URL) throw new Error("The hosted page needs VITE_RELAY_URL: the relay's address.");

  return {
    root: uiRoot,
    // Pages that go on the website as they are: the privacy policy.
    publicDir: hosted ? "hosted-static" : false,
    plugins: [
      vue(),
      hosted && {
        name: "hosted-page",
        closeBundle() {
          // The helper page is the site's only page, whatever the rest of the address says: the
          // rest names the sharer.
          renameSync(`${uiRoot}hosted/dist/helper.html`, `${uiRoot}hosted/dist/index.html`);
          writeFileSync(`${uiRoot}hosted/dist/_redirects`, "/* /index.html 200\n");
          // What the page looks at to know it is out of date, and the app to know a newer one is out.
          writeFileSync(`${uiRoot}hosted/dist/version.json`, JSON.stringify({ version, build }) + "\n");
        },
      },
    ],
    build: {
      outDir: hosted ? "hosted/dist" : "dist",
      emptyOutDir: true,
      rollupOptions: {
        // Three pages: the console and the on-screen indicator (this computer only), and the helper
        // page (served on the LAN).
        input: hosted ? [`${uiRoot}helper.html`] : [`${uiRoot}index.html`, `${uiRoot}indicator.html`, `${uiRoot}helper.html`],
      },
    },
  };
});
