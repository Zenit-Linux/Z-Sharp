import { build } from "esbuild";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import assert from "node:assert/strict";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tmp = mkdtempSync(join(tmpdir(), "zs-site-test-"));
const entry = join(tmp, "entry.ts");
writeFileSync(entry, [
  `export * from ${JSON.stringify(join(root, "src/router.ts"))};`,
  `export * from ${JSON.stringify(join(root, "src/highlight.ts"))};`,
  `export * from ${JSON.stringify(join(root, "src/markdown.ts"))};`,
  `export * from ${JSON.stringify(join(root, "src/data.ts"))};`,
  `export { mdLinkToRoute } from ${JSON.stringify(join(root, "src/docs.ts"))};`,
].join("\n"));
const out = join(tmp, "entry.mjs");
await build({ entryPoints: [entry], outfile: out, bundle: true, format: "esm", platform: "node", logLevel: "silent" });
const m = await import(pathToFileURL(out).href);

let passed = 0;
const t = (name, fn) => { try { fn(); passed++; console.log("ok   " + name); } catch (e) { console.error("FAIL " + name + "\n  " + (e && e.message)); process.exitCode = 1; } };

t("router: trasy", () => {
  assert.deepEqual(m.parseRoute(""), { page: "home" });
  assert.deepEqual(m.parseRoute("#/docs/spec#sekcja"), { page: "docs", doc: "spec", anchor: "sekcja" });
  assert.deepEqual(m.parseRoute("#/stdlib/Std.Json"), { page: "stdlib", module: "Std.Json" });
  assert.deepEqual(m.parseRoute("#/playground/closures"), { page: "playground", example: "closures" });
  assert.deepEqual(m.parseRoute("#/nieznane"), { page: "home" });
  assert.equal(m.href({ page: "docs", doc: "spec", anchor: null }), "#/docs/spec");
});

t("lekser: konkatenacja tokenów odtwarza wejście", () => {
  for (const ex of m.EXAMPLES) assert.equal(m.tokenize(ex.code).map((x) => x.text).join(""), ex.code, ex.id);
  const src = 'extern "c" {\n    func getpid(): int;\n}\n$x = fn($a) => $a ** 2; # komentarz\n##\n doc\n##\nprintf("%s", 1.5e3);';
  assert.equal(m.tokenize(src).map((x) => x.text).join(""), src);
});

t("lekser: rodzaje tokenów", () => {
  const kinds = (s) => m.tokenize(s).filter((x) => x.kind !== "space").map((x) => x.kind + ":" + x.text);
  assert.ok(kinds("extern").includes("keyword:extern"));
  assert.ok(kinds("$abc").some((x) => x.startsWith("variable:")));
  assert.ok(kinds("true").includes("constant:true"));
  assert.ok(kinds('"a"').some((x) => x.startsWith("string:")));
  assert.ok(kinds("# c").some((x) => x.startsWith("comment:")));
});

t("highlightHtml: escapuje HTML", () => {
  const h = m.highlightHtml('echo "<script>alert(1)</script>";');
  assert.ok(!h.includes("<script>"), h);
});

t("markdown: nagłówki, kod, tabele, linki", () => {
  const r = m.renderMarkdown("# Tytuł\n\n## Sekcja A\n\ntekst `kod` i **b**\n\n```zsharp\necho 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[spec](SPEC.md#x)\n");
  assert.deepEqual(r.headings.map((x) => x.text), ["Tytuł", "Sekcja A"]);
  assert.match(r.html, /<table/);
  assert.match(r.html, /<strong>b<\/strong>/);
});

t("markdown: niebezpieczne linki są neutralizowane", () => {
  const r = m.renderMarkdown("[x](javascript:alert(1))");
  assert.ok(!/href="javascript:/i.test(r.html), r.html);
});

t("dane: dokumenty, przykłady, biblioteka", () => {
  assert.ok(m.DOCS.length >= 8);
  assert.equal(new Set(m.DOCS.map((d) => d.id)).size, m.DOCS.length);
  assert.equal(new Set(m.EXAMPLES.map((d) => d.id)).size, m.EXAMPLES.length);
  assert.ok(m.STDLIB.every((x) => x.functions.length > 0));
  assert.equal(m.mdLinkToRoute("SPEC.md#x"), "#/docs/spec#x");
  assert.equal(m.mdLinkToRoute("../docs/STATUS.md"), "#/docs/status");
  assert.equal(m.mdLinkToRoute("https://example.com/"), null);
});

rmSync(tmp, { recursive: true, force: true });
console.log(`\n${passed} testów przeszło`);
