// Checks that every language has every text the app says, and nothing it no longer says.
// Run from the repository root: node scripts/check-i18n.mjs  (also part of `pnpm typecheck`).
// With --list it prints the texts instead, one a line, for starting a new language.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const source = "apps/ui/src";
const files = (folder) =>
  readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    return statSync(path).isDirectory() ? (name === "locales" ? [] : files(path)) : /\.(ts|vue)$/.test(name) ? [path] : [];
  });

// A text is the first argument of t(…), written as a plain string in either kind of quotes.
const call = /\bt\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')/g;
const said = new Set();
for (const file of files(source)) {
  for (const match of readFileSync(file, "utf8").matchAll(call)) {
    said.add((match[1] ?? match[2]).replace(/\\(.)/g, "$1"));
  }
}

if (process.argv.includes("--list")) {
  for (const text of [...said].sort()) console.log(JSON.stringify(text));
  process.exit(0);
}

let problems = 0;
for (const name of readdirSync(join(source, "locales")).filter((file) => file.endsWith(".ts"))) {
  const language = name.replace(/\.ts$/, "");
  const catalog = (await import(`../${source}/locales/${name}`))[language];
  const missing = [...said].filter((text) => !(text in catalog));
  const unused = Object.keys(catalog).filter((text) => !said.has(text));
  // What stands for something in a text has to be there in its translation, or it is lost.
  const broken = Object.entries(catalog).filter(([text, translated]) => {
    const names = (value) => (value.match(/\{\w+\}/g) ?? []).sort().join();
    return said.has(text) && names(text) !== names(translated);
  });
  for (const text of missing) console.log(`${language}: not translated: ${JSON.stringify(text)}`);
  for (const text of unused) console.log(`${language}: no longer said: ${JSON.stringify(text)}`);
  for (const [text] of broken) console.log(`${language}: different {names} in: ${JSON.stringify(text)}`);
  problems += missing.length + unused.length + broken.length;
  console.log(`${language}: ${Object.keys(catalog).length} texts, ${missing.length} missing, ${unused.length} unused, ${broken.length} with different names`);
}
process.exit(problems ? 1 : 0);
