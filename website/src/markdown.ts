import { escapeHtml, highlightHtml } from "./highlight";

export interface Heading { level: number; text: string; id: string; }
export interface Rendered { html: string; headings: Heading[]; }

export function slugify(s: string): string {
  const base = s.toLowerCase().replace(/<[^>]+>/g, "").replace(/[`*_~]/g, "").trim()
    .replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-+|-+$/g, "");
  return base === "" ? "sekcja" : base;
}

function safeHref(href: string): string {
  const h = href.trim();
  if (/^(https?:|mailto:|#|\.{0,2}\/|[\w./-]+\.md)/i.test(h)) return h;
  return "#";
}

/** Tekst wbudowany: `kod`, **pogrubienie**, *kursywa*, [tekst](url). */
export function inline(raw: string): string {
  const codes: string[] = [];
  let s = raw.replace(/`([^`]+)`/g, (_m, c: string) => { codes.push(`<code>${escapeHtml(c)}</code>`); return `\u0000${codes.length - 1}\u0000`; });
  s = escapeHtml(s);
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_m, text: string, href: string) => {
    const url = safeHref(href.replace(/&amp;/g, "&"));
    const external = /^https?:/i.test(url);
    return `<a href="${escapeHtml(url)}"${external ? ' target="_blank" rel="noopener noreferrer"' : ""}>${text}</a>`;
  });
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>").replace(/(^|[\s(])\*([^*\s][^*]*)\*(?=$|[\s).,;:!?])/g, "$1<em>$2</em>");
  return s.replace(/\u0000(\d+)\u0000/g, (_m, i: string) => codes[Number(i)] ?? "");
}

function splitRow(line: string): string[] {
  return line.trim().replace(/^\|/, "").replace(/\|$/, "").split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
}

interface ListItem { indent: number; ordered: boolean; text: string; }

/** Lista z zagnieżdżeniem wyznaczanym wcięciem; zwraca HTML i indeks pierwszego nieprzetworzonego elementu. */
function renderList(items: ListItem[], start: number, indent: number): { html: string; next: number } {
  const ordered = items[start]?.ordered ?? false;
  const tag = ordered ? "ol" : "ul";
  let html = `<${tag}>`;
  let k = start;
  while (k < items.length) {
    const it = items[k] as ListItem;
    if (it.indent < indent) break;
    if (it.indent > indent) {   // zagnieżdżenie bez rodzica — traktuj jak ten sam poziom
      const sub = renderList(items, k, it.indent);
      html += sub.html; k = sub.next; continue;
    }
    html += `<li>${inline(it.text)}`;
    k++;
    const nxt = items[k];
    if (nxt && nxt.indent > indent) { const sub = renderList(items, k, nxt.indent); html += sub.html; k = sub.next; }
    html += "</li>";
  }
  return { html: html + `</${tag}>`, next: k };
}

export function renderMarkdown(src: string): Rendered {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const out: string[] = [];
  const headings: Heading[] = [];
  const used = new Map<string, number>();
  let i = 0;
  const uniqueId = (text: string): string => {
    const b = slugify(text);
    const k = used.get(b) ?? 0;
    used.set(b, k + 1);
    return k === 0 ? b : `${b}-${k}`;
  };

  while (i < lines.length) {
    const line = lines[i] as string;
    if (line.trim() === "") { i++; continue; }

    const fence = /^```(\S*)\s*$/.exec(line);
    if (fence) {
      const lang = (fence[1] ?? "").toLowerCase();
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i] as string)) { body.push(lines[i] as string); i++; }
      i++;
      const code = body.join("\n");
      const html = ["php", "zs", "zsharp", "z#"].includes(lang) ? highlightHtml(code) : escapeHtml(code);
      out.push(`<pre class="code" data-lang="${escapeHtml(lang || "text")}"><code>${html}</code></pre>`);
      continue;
    }
    const h = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (h) {
      const level = (h[1] as string).length;
      const text = (h[2] as string).replace(/`/g, "");
      const id = uniqueId(text);
      headings.push({ level, text, id });
      out.push(`<h${level} id="${id}">${inline(h[2] as string)}</h${level}>`);
      i++; continue;
    }
    if (/^(-{3,}|\*{3,})\s*$/.test(line)) { out.push("<hr>"); i++; continue; }
    if (line.startsWith(">")) {
      const q: string[] = [];
      while (i < lines.length && (lines[i] as string).startsWith(">")) { q.push((lines[i] as string).replace(/^>\s?/, "")); i++; }
      out.push(`<blockquote>${renderMarkdown(q.join("\n")).html}</blockquote>`);
      continue;
    }
    if (line.includes("|") && /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(lines[i + 1] ?? "")) {
      const head = splitRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && (lines[i] as string).includes("|") && (lines[i] as string).trim() !== "") { rows.push(splitRow(lines[i] as string)); i++; }
      out.push(`<div class="table-wrap"><table><thead><tr>${head.map((c) => `<th>${inline(c)}</th>`).join("")}</tr></thead><tbody>${
        rows.map((r) => `<tr>${head.map((_c, k) => `<td>${inline(r[k] ?? "")}</td>`).join("")}</tr>`).join("")}</tbody></table></div>`);
      continue;
    }
    const li = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(line);
    if (li) {
      const items: ListItem[] = [];
      while (i < lines.length) {
        const m = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(lines[i] as string);
        if (m) { items.push({ indent: (m[1] as string).replace(/\t/g, "    ").length, ordered: /\d/.test(m[2] as string), text: m[3] as string }); i++; continue; }
        if (/^\s{2,}\S/.test(lines[i] as string) && items.length > 0) { (items[items.length - 1] as ListItem).text += " " + (lines[i] as string).trim(); i++; continue; }
        break;
      }
      out.push(renderList(items, 0, items[0]?.indent ?? 0).html);
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && (lines[i] as string).trim() !== "" && !/^(```|#{1,6}\s|>|\s*([-*]|\d+\.)\s)/.test(lines[i] as string)) { para.push(lines[i] as string); i++; }
    if (para.length === 0) { para.push(line); i++; }
    out.push(`<p>${inline(para.join(" "))}</p>`);
  }
  return { html: out.join("\n"), headings };
}
