import { h, html, clear } from "./dom";
import { DOCS, EXAMPLES, MILESTONES, STDLIB, type Health } from "./data";
import { highlightHtml, stats } from "./highlight";
import { loadDoc, mdLinkToRoute } from "./docs";
import { href } from "./router";

const HEALTH_LABEL: Record<Health, string> = { done: "✅ gotowe", partial: "🟡 częściowo", planned: "⏳ planowane" };

function codeBlock(src: string, lang = "zsharp"): HTMLElement {
  const copy = h("button", { class: "copy", type: "button", "aria-label": "Kopiuj kod" }, "Kopiuj");
  copy.addEventListener("click", () => {
    void navigator.clipboard?.writeText(src).then(() => { copy.textContent = "Skopiowano"; setTimeout(() => { copy.textContent = "Kopiuj"; }, 1400); });
  });
  const pre = html("pre", "code", `<code>${highlightHtml(src)}</code>`);
  pre.dataset["lang"] = lang;
  return h("div", { class: "codewrap" }, copy, pre);
}

export function homePage(root: HTMLElement): void {
  clear(root);
  root.append(
    h("section", { class: "hero" },
      h("h1", {}, "Z#"),
      h("p", { class: "lead" }, "Dynamicznie typowany język ogólnego przeznaczenia o składni znajomej z PHP — z własnym interpreterem, kompilatorem natywnym (C/LLVM) i biblioteką standardową."),
      h("div", { class: "cta" },
        h("a", { class: "btn primary", href: href({ page: "docs", doc: "spec", anchor: null }) }, "Specyfikacja"),
        h("a", { class: "btn", href: href({ page: "playground", example: "hello" }) }, "Przykłady"),
        h("a", { class: "btn", href: href({ page: "status" }) }, "Status projektu")),
    ),
    h("section", { class: "grid" },
      ...[
        ["Interpreter i kompilator", "zsi uruchamia kod, zsc kompiluje do natywnego C lub LLVM; zsc skompilowany natywnie kompiluje samego siebie (stage3)."],
        ["Pamięć bez wycieków", "Runtime natywny zwalnia nieosiągalne wartości (M4): programy długo żyjące nie rosną w nieskończoność."],
        ["Idiomatyczny kod", "Domknięcia, funkcje strzałkowe, argumenty nazwane, spread w wywołaniach i literałach tablic, klasy i wyjątki."],
        ["Biblioteka standardowa", "Json, Toml, Regex, Time, Path, Os i Net (TCP/UDP/HTTP) — jednakowe w interpreterze i w kodzie natywnym."],
      ].map(([t, d]) => h("article", { class: "card" }, h("h3", {}, t as string), h("p", {}, d as string))),
    ),
    h("section", {}, h("h2", {}, "Szybki start"), codeBlock(EXAMPLES[0]?.code ?? "")),
    h("section", {}, h("h2", {}, "Dokumentacja"),
      h("ul", { class: "doclist" }, ...DOCS.map((d) => h("li", {}, h("a", { href: href({ page: "docs", doc: d.id, anchor: null }) }, d.title), ` — ${d.summary}`)))),
  );
}

export async function docsPage(root: HTMLElement, docId: string | null, anchor: string | null): Promise<void> {
  clear(root);
  if (docId === null) {
    root.append(h("h1", {}, "Dokumentacja"), h("ul", { class: "doclist" },
      ...DOCS.map((d) => h("li", {}, h("a", { href: href({ page: "docs", doc: d.id, anchor: null }) }, d.title), ` — ${d.summary}`))));
    return;
  }
  root.append(h("p", { class: "muted" }, "Ładowanie…"));
  try {
    const doc = await loadDoc(docId);
    clear(root);
    if (!doc) { root.append(h("h1", {}, "Nie znaleziono dokumentu"), h("a", { href: "#/docs" }, "← wszystkie dokumenty")); return; }
    const toc = h("nav", { class: "toc", "aria-label": "Spis treści" },
      h("strong", {}, "Spis treści"),
      h("ul", {}, ...doc.rendered.headings.filter((x) => x.level >= 2 && x.level <= 3).map((x) =>
        h("li", { class: `lvl${x.level}` }, h("a", { href: `#/docs/${docId}#${x.id}` }, x.text)))));
    const body = html("article", "prose", doc.rendered.html);
    body.querySelectorAll("a[href]").forEach((a) => {
      const target = mdLinkToRoute(a.getAttribute("href") ?? "");
      if (target) a.setAttribute("href", target);
    });
    root.append(h("div", { class: "doc-layout" }, toc, body));
    if (anchor) document.getElementById(anchor)?.scrollIntoView();
  } catch (e) {
    clear(root);
    root.append(h("h1", {}, "Nie udało się wczytać dokumentu"), h("p", { class: "error" }, String(e instanceof Error ? e.message : e)),
      h("p", {}, "Zbuduj stronę poleceniem ", h("code", {}, "npm run build"), " w katalogu website/ (kopiuje dokumenty do dist/content)."));
  }
}

export function stdlibPage(root: HTMLElement, moduleName: string | null): void {
  clear(root);
  const input = h("input", { type: "search", placeholder: "Szukaj funkcji (np. parseIso, readFile)…", "aria-label": "Szukaj w bibliotece standardowej" });
  const list = h("div", { class: "modules" });
  const render = (): void => {
    const q = (input as HTMLInputElement).value.trim().toLowerCase();
    clear(list);
    for (const m of STDLIB) {
      if (moduleName && m.name !== moduleName) continue;
      const fns = q === "" ? m.functions : m.functions.filter((f) => f.toLowerCase().includes(q) || m.name.toLowerCase().includes(q));
      if (fns.length === 0) continue;
      const alias = m.name.split(".")[1] ?? m.name;
      list.append(h("article", { class: "card module", id: m.name },
        h("h3", {}, h("a", { href: `#/stdlib/${m.name}` }, m.name)), h("p", {}, m.summary),
        h("p", { class: "muted" }, "Plik: ", h("code", {}, m.file)),
        h("div", { class: "chips" }, ...fns.map((f) => h("code", { class: "chip", title: `${alias}.${f}(…)` }, `${alias}.${f}`)))));
    }
    if (list.childElementCount === 0) list.append(h("p", { class: "muted" }, "Brak wyników."));
  };
  input.addEventListener("input", render);
  render();
  root.append(h("h1", {}, "Biblioteka standardowa"),
    h("p", {}, "Import: ", h("code", {}, "import Std.Json;"), " — nazwą modułu w kodzie jest ostatni segment (", h("code", {}, "Json.decode(…)"), ")."),
    codeBlock("import Std.Json;\n\n$v = Json.decode(\"[1, 2, 3]\");\necho count($v);"),
    ...(moduleName ? [h("p", {}, h("a", { href: "#/stdlib" }, "← wszystkie moduły"))] : []), input, list,
    h("p", {}, "Pełny opis: ", h("a", { href: href({ page: "docs", doc: "stdlib", anchor: null }) }, "dokumentacja biblioteki standardowej"), "."));
}

export function statusPage(root: HTMLElement): void {
  clear(root);
  const done = MILESTONES.filter((m) => m.health === "done").length;
  root.append(
    h("h1", {}, "Status projektu"),
    h("p", { class: "lead" }, `Ukończone kamienie milowe: ${done} z ${MILESTONES.length}.`),
    h("div", { class: "progress", role: "progressbar", "aria-valuemin": "0", "aria-valuemax": String(MILESTONES.length), "aria-valuenow": String(done) },
      h("div", { class: "fill", style: `width:${Math.round((done / MILESTONES.length) * 100)}%` })),
    ...MILESTONES.map((m) => h("article", { class: `card milestone ${m.health}`, id: m.id },
      h("h3", {}, `${m.id} — ${m.title} `, h("span", { class: "badge" }, HEALTH_LABEL[m.health])),
      h("p", { class: "muted" }, `Odblokowuje: ${m.unlocks}`),
      m.done.length ? h("ul", { class: "done" }, ...m.done.map((x) => h("li", {}, x))) : null,
      m.todo.length ? h("ul", { class: "todo" }, ...m.todo.map((x) => h("li", {}, x))) : null)),
    h("p", {}, "Szczegóły: ", h("a", { href: href({ page: "docs", doc: "status", anchor: null }) }, "STATUS.md"), "."));
}

export function playgroundPage(root: HTMLElement, exampleId: string | null): void {
  clear(root);
  const ex = EXAMPLES.find((e) => e.id === exampleId) ?? EXAMPLES[0];
  const area = h("textarea", { class: "editor", spellcheck: "false", "aria-label": "Kod Z#", rows: "14" }) as HTMLTextAreaElement;
  area.value = ex?.code ?? "";
  const out = html("pre", "code preview", "");
  const info = h("p", { class: "muted" });
  const update = (): void => {
    out.innerHTML = `<code>${highlightHtml(area.value)}</code>`;
    const s = stats(area.value);
    info.textContent = `${s.lines} linii · ${s.tokens} tokenów · ${s.variables} zmiennych · ${s.functions} funkcji · ${s.comments} komentarzy · ${s.strings} napisów`;
  };
  area.addEventListener("input", update);
  update();
  root.append(h("h1", {}, "Przykłady i podświetlanie składni"),
    h("p", {}, "Wklej własny kod Z# — kolorowanie działa w przeglądarce (lekser w TypeScript). Uruchamianie: ", h("code", {}, "zsi plik.zsharp"), "."),
    h("div", { class: "tabs", role: "tablist" }, ...EXAMPLES.map((e) =>
      h("a", { class: "tab" + (e.id === ex?.id ? " active" : ""), role: "tab", href: href({ page: "playground", example: e.id }) }, e.title))),
    ...(ex ? [h("p", { class: "muted" }, ex.description)] : []),
    h("div", { class: "split" }, area, out), info);
}
