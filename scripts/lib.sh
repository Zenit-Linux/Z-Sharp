# wspólne dla skryptów: ROOT, ZSC0, lokalizacja repo zsc
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ZS_HOME="$ROOT"
ZSC0="${ZSC0:-$ROOT/target/release/zsc0}"
ZSC_DIR="${ZSC_DIR:-}"
if [ -z "$ZSC_DIR" ]; then for c in "$ROOT/../zsc" "$ROOT/zsc"; do [ -f "$c/src/main.zsharp" ] && ZSC_DIR="$(cd "$c" && pwd)" && break; done; fi
ensure_stage0() { [ -x "$ZSC0" ] || (cd "$ROOT" && cargo build --release --quiet) || { echo "nie udało się zbudować stage0"; exit 1; }; }
# repo zsc oczekuje katalogu Z-Sharp/ obok src/ — jeśli go brak, tworzymy symlink tymczasowy
link_workspace() { [ -e "$ZSC_DIR/Z-Sharp" ] || { ln -s "$ROOT" "$ZSC_DIR/Z-Sharp"; CREATED_LINK=1; }; }
unlink_workspace() { [ "${CREATED_LINK:-0}" = 1 ] && rm -f "$ZSC_DIR/Z-Sharp"; true; }
