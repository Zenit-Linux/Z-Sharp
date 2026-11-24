type Child = Node | string | null | undefined | false;

/** Mały pomocnik budowania DOM bez innerHTML (tekst jest zawsze wstawiany jako tekst). */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K, attrs: Record<string, string | boolean | ((e: Event) => void)> = {}, ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (typeof v === "function") el.addEventListener(k.replace(/^on/, ""), v);
    else if (v === true) el.setAttribute(k, "");
    else if (v !== false) el.setAttribute(k, v);
  }
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    el.append(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return el;
}

/** Wstawia zaufany (własny, już escapowany) HTML — wyłącznie z renderMarkdown/highlightHtml. */
export function html(tag: keyof HTMLElementTagNameMap, cls: string, markup: string): HTMLElement {
  const el = document.createElement(tag);
  el.className = cls;
  el.innerHTML = markup;
  return el;
}

export function clear(el: Element): void { while (el.firstChild) el.removeChild(el.firstChild); }
