export type Theme = "light" | "dark";
const KEY = "zsharp-theme";

export function currentTheme(): Theme {
  try { const s = localStorage.getItem(KEY); if (s === "light" || s === "dark") return s; } catch { /* brak dostępu do storage */ }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(t: Theme): void {
  document.documentElement.dataset["theme"] = t;
  try { localStorage.setItem(KEY, t); } catch { /* ignoruj */ }
}

export function toggleTheme(): Theme {
  const next: Theme = currentTheme() === "dark" ? "light" : "dark";
  applyTheme(next);
  return next;
}
