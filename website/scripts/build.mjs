import { build, context } from "esbuild";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(root, "..");
const dist = join(root, "dist");
const watch = process.argv.includes("--watch");
const minify = process.argv.includes("--minify");

function collectContent() {
  const files = {};
  const docs = join(repo, "docs");
  if (existsSync(docs)) {
    for (const f of readdirSync(docs)) if (f.endsWith(".md")) files[f] = readFileSync(join(docs, f), "utf8");
  }
  const cl = join(repo, "CHANGELOG.md");
  if (existsSync(cl)) files["CHANGELOG.md"] = readFileSync(cl, "utf8");
  return files;
}

function prepareDist(content) {
  rmSync(dist, { recursive: true, force: true });
  mkdirSync(join(dist, "content"), { recursive: true });
  for (const [name, text] of Object.entries(content)) writeFileSync(join(dist, "content", name), text);
}

const content = collectContent();
prepareDist(content);
// "</script" w treści nie psuje pliku .js, ale zabezpieczamy się przed U+2028/2029 w literałach
const embedded = JSON.stringify(content).replace(/\u2028/g, "\\u2028").replace(/\u2029/g, "\\u2029");

const options = {
  entryPoints: { app: join(root, "src/main.ts"), "app-css": join(root, "src/styles.css") },
  outdir: dist,
  bundle: true,
  format: "iife",
  target: "es2020",
  minify,
  sourcemap: false,
  logLevel: "info",
  banner: { js: `window.__ZS_CONTENT__=${embedded};` },
  entryNames: "[name]",
};

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
  console.log("build: obserwuję zmiany (Ctrl+C kończy)");
} else {
  await build(options);
  // esbuild nazwał wynik CSS "app-css.css" — publikujemy jako app.css; pusty app-css.js usuwamy
  const css = join(dist, "app-css.css");
  if (existsSync(css)) cpSync(css, join(dist, "app.css"));
  rmSync(css, { force: true });
  rmSync(join(dist, "app-css.js"), { force: true });
  for (const need of ["app.js", "app.css"]) {
    if (!existsSync(join(dist, need))) { console.error(`build: brak dist/${need}`); process.exit(1); }
  }
  console.log(`build: ok — dist/app.js, dist/app.css, ${Object.keys(content).length} dokumentów`);
}
