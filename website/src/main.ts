import { applyTheme, currentTheme, toggleTheme } from "./theme";
import { parseRoute } from "./router";
import { docsPage, homePage, playgroundPage, statusPage, stdlibPage } from "./pages";

function render(): void {
  const root = document.getElementById("app");
  if (!root) return;
  const r = parseRoute(location.hash);
  document.querySelectorAll<HTMLAnchorElement>("nav.main a[data-page]").forEach((a) => {
    a.classList.toggle("active", a.dataset["page"] === r.page);
    if (a.dataset["page"] === r.page) a.setAttribute("aria-current", "page"); else a.removeAttribute("aria-current");
  });
  const titles: Record<string, string> = { home: "Z# — język programowania", docs: "Dokumentacja", stdlib: "Biblioteka standardowa", status: "Status", playground: "Przykłady" };
  document.title = r.page === "home" ? (titles["home"] as string) : `${titles[r.page]} · Z#`;
  switch (r.page) {
    case "home": homePage(root); break;
    case "docs": void docsPage(root, r.doc, r.anchor); break;
    case "stdlib": stdlibPage(root, r.module); break;
    case "status": statusPage(root); break;
    case "playground": playgroundPage(root, r.example); break;
  }
  if (r.page !== "docs" || !r.anchor) window.scrollTo(0, 0);
}

function init(): void {
  applyTheme(currentTheme());
  document.getElementById("theme-toggle")?.addEventListener("click", () => {
    const t = toggleTheme();
    const b = document.getElementById("theme-toggle");
    if (b) b.textContent = t === "dark" ? "☀️ Jasny" : "🌙 Ciemny";
  });
  const b = document.getElementById("theme-toggle");
  if (b) b.textContent = currentTheme() === "dark" ? "☀️ Jasny" : "🌙 Ciemny";
  window.addEventListener("hashchange", render);
  render();
}

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init); else init();
