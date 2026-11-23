export type Route =
  | { page: "home" }
  | { page: "docs"; doc: string | null; anchor: string | null }
  | { page: "stdlib"; module: string | null }
  | { page: "status" }
  | { page: "playground"; example: string | null };

/** `#/docs/spec#sekcja`, `#/stdlib/Std.Json`, `#/playground/closures`, `#/status`; nieznane → home. */
export function parseRoute(hash: string): Route {
  const h = hash.replace(/^#\/?/, "");
  const [pathPart = "", anchor = null] = h.split("#", 2) as [string, string?];
  const seg = pathPart.split("/").filter((s) => s !== "").map((s) => decodeURIComponent(s));
  switch (seg[0]) {
    case "docs": return { page: "docs", doc: seg[1] ?? null, anchor: anchor ?? null };
    case "stdlib": return { page: "stdlib", module: seg[1] ?? null };
    case "status": return { page: "status" };
    case "playground": return { page: "playground", example: seg[1] ?? null };
    default: return { page: "home" };
  }
}

export function href(r: Route): string {
  switch (r.page) {
    case "home": return "#/";
    case "docs": return `#/docs${r.doc ? "/" + encodeURIComponent(r.doc) : ""}${r.anchor ? "#" + r.anchor : ""}`;
    case "stdlib": return `#/stdlib${r.module ? "/" + encodeURIComponent(r.module) : ""}`;
    case "status": return "#/status";
    case "playground": return `#/playground${r.example ? "/" + encodeURIComponent(r.example) : ""}`;
  }
}
