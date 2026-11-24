export type TokenKind =
  | "comment" | "doc" | "string" | "variable" | "number" | "keyword" | "constant"
  | "module" | "function" | "operator" | "punct" | "ident" | "space" | "shebang";

export interface Token { kind: TokenKind; text: string; }

export const KEYWORDS: ReadonlySet<string> = new Set([
  "func", "function", "return", "if", "elseif", "else", "switch", "case", "default", "match",
  "for", "foreach", "while", "do", "break", "continue", "as", "import", "namespace", "extern",
  "class", "interface", "trait", "extends", "implements", "new", "public", "private", "protected",
  "static", "abstract", "const", "try", "catch", "finally", "throw", "using", "await", "echo",
  "necho", "eecho", "and", "or", "xor", "instanceof", "fn",
]);
export const CONSTANTS: ReadonlySet<string> = new Set(["true", "false", "null"]);
export const MODULES: ReadonlySet<string> = new Set([
  "System", "Math", "File", "Directory", "Process", "Task", "Socket", "Std", "Workspace",
]);

const OPERATORS = [
  "===", "!==", "??=", "...", "<<=", ">>=", "**=", "==", "!=", "<=", ">=", "&&", "||", "??", "->", "=>",
  "++", "--", "+=", "-=", "*=", "/=", ".=", "%=", "<<", ">>", "**", "+", "-", "*", "/", "%", "=", "<", ">",
  "!", "&", "|", "^", "~", "?", ":", ".",
];

const isDigit = (c: string): boolean => c >= "0" && c <= "9";
const isIdStart = (c: string): boolean => (c >= "a" && c <= "z") || (c >= "A" && c <= "Z") || c === "_" || c > "\u007f";
const isIdPart = (c: string): boolean => isIdStart(c) || isDigit(c);

/** Dzieli kod na tokeny; konkatenacja `text` wszystkich tokenów odtwarza wejście bez zmian. */
export function tokenize(src: string): Token[] {
  const out: Token[] = [];
  const n = src.length;
  let i = 0;
  const push = (kind: TokenKind, text: string): void => { out.push({ kind, text }); };

  if (src.startsWith("#!")) {
    const e = src.indexOf("\n");
    const end = e < 0 ? n : e;
    push("shebang", src.slice(0, end));
    i = end;
  }
  while (i < n) {
    const c = src[i] as string;
    if (c === " " || c === "\t" || c === "\n" || c === "\r") {
      let j = i + 1;
      while (j < n && " \t\n\r".includes(src[j] as string)) j++;
      push("space", src.slice(i, j)); i = j; continue;
    }
    if (c === "#") {
      if (src.startsWith("#(", i)) {
        const e = src.indexOf(")#", i + 2);
        const end = e < 0 ? n : e + 2;
        push("comment", src.slice(i, end)); i = end; continue;
      }
      if (src.startsWith("##", i)) {
        const e = src.indexOf("##", i + 2);
        const end = e < 0 ? n : e + 2;
        push("doc", src.slice(i, end)); i = end; continue;
      }
      let j = i;
      while (j < n && src[j] !== "\n") j++;
      push("comment", src.slice(i, j)); i = j; continue;
    }
    if (c === "\"" || c === "'") {
      let j = i + 1;
      while (j < n && src[j] !== c) { if (src[j] === "\\" && j + 1 < n) j++; j++; }
      j = Math.min(n, j + 1);
      push("string", src.slice(i, j)); i = j; continue;
    }
    if (c === "$" && i + 1 < n && isIdStart(src[i + 1] as string)) {
      let j = i + 1;
      while (j < n && isIdPart(src[j] as string)) j++;
      push("variable", src.slice(i, j)); i = j; continue;
    }
    if (isDigit(c)) {
      let j = i + 1;
      if (c === "0" && /[xXbBoO]/.test(src[j] ?? "")) { j++; while (j < n && /[0-9a-fA-F_]/.test(src[j] as string)) j++; }
      else {
        while (j < n && (isDigit(src[j] as string) || src[j] === "_")) j++;
        if (src[j] === "." && isDigit(src[j + 1] ?? "")) { j++; while (j < n && (isDigit(src[j] as string) || src[j] === "_")) j++; }
        if (/[eE]/.test(src[j] ?? "") && /[0-9+-]/.test(src[j + 1] ?? "")) { j += 2; while (j < n && isDigit(src[j] as string)) j++; }
      }
      push("number", src.slice(i, j)); i = j; continue;
    }
    if (isIdStart(c)) {
      let j = i + 1;
      while (j < n && isIdPart(src[j] as string)) j++;
      const word = src.slice(i, j);
      let k = j;
      while (k < n && (src[k] === " " || src[k] === "\t")) k++;
      const prev = out.filter((t) => t.kind !== "space").at(-1);
      const afterFunc = prev?.kind === "keyword" && (prev.text === "func" || prev.text === "function" || prev.text === "class" || prev.text === "interface");
      if (KEYWORDS.has(word)) push("keyword", word);
      else if (CONSTANTS.has(word)) push("constant", word);
      else if (MODULES.has(word) || (/^[A-Z]/.test(word) && src[k] === "." && prev?.text !== "->")) push("module", word);
      else if (src[k] === "(" || afterFunc) push("function", word);
      else push("ident", word);
      i = j; continue;
    }
    const op = OPERATORS.find((o) => src.startsWith(o, i));
    if (op !== undefined) { push("operator", op); i += op.length; continue; }
    push("punct", c); i++;
  }
  return out;
}

export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

/** Kod → HTML (`<span class="tok-…">`); wejście jest zawsze escapowane. */
export function highlightHtml(src: string): string {
  return tokenize(src)
    .map((t) => (t.kind === "space" || t.kind === "punct" || t.kind === "ident" ? escapeHtml(t.text) : `<span class="tok-${t.kind}">${escapeHtml(t.text)}</span>`))
    .join("");
}

export interface Stats { lines: number; tokens: number; functions: number; variables: number; comments: number; strings: number; }

export function stats(src: string): Stats {
  const toks = tokenize(src);
  const vars = new Set<string>();
  const funcs = new Set<string>();
  let comments = 0, strings = 0;
  for (const t of toks) {
    if (t.kind === "variable") vars.add(t.text);
    else if (t.kind === "function") funcs.add(t.text);
    else if (t.kind === "comment" || t.kind === "doc") comments++;
    else if (t.kind === "string") strings++;
  }
  return {
    lines: src === "" ? 0 : src.split("\n").length,
    tokens: toks.filter((t) => t.kind !== "space").length,
    functions: funcs.size, variables: vars.size, comments, strings,
  };
}
