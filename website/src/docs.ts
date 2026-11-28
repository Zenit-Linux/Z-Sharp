import { DOCS } from "./data";
import { renderMarkdown, type Rendered } from "./markdown";

declare global { interface Window { __ZS_CONTENT__?: Record<string, string>; } }

const cache = new Map<string, Promise<string>>();

/** Dokumenty są kopiowane przez scripts/build.mjs do website/dist/content/. Ścieżka względem index.html. */
export const CONTENT_BASE = "website/dist/content/";

export function loadText(file: string): Promise<string> {
  // dokumenty wbudowane przez scripts/build.mjs — działa bez serwera (file://) i bez dodatkowych żądań
  const embedded = typeof window !== "undefined" ? window.__ZS_CONTENT__?.[file] : undefined;
  if (embedded !== undefined) return Promise.resolve(embedded);
  let p = cache.get(file);
  if (!p) {
    p = fetch(CONTENT_BASE + file).then((r) => {
      if (!r.ok) throw new Error(`${file}: HTTP ${r.status}`);
      return r.text();
    });
    cache.set(file, p);
    p.catch(() => cache.delete(file));
  }
  return p;
}

export async function loadDoc(id: string): Promise<{ title: string; rendered: Rendered } | null> {
  const d = DOCS.find((x) => x.id === id);
  if (!d) return null;
  const text = await loadText(d.file);
  return { title: d.title, rendered: renderMarkdown(text) };
}

/** Mapuje względny link `*.md` z dokumentów na trasę strony. */
export function mdLinkToRoute(link: string): string | null {
  const m = /(?:^|\/)([A-Z]+)\.md(#.*)?$/.exec(link);
  if (!m) return null;
  const d = DOCS.find((x) => x.file === `${m[1]}.md`);
  return d ? `#/docs/${d.id}${m[2] ?? ""}` : null;
}
