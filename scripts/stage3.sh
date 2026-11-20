#!/usr/bin/env bash
# stage3: natywna samokompilacja zsc (bez interpretera w kompilatorze).
#   stage2: zsc0 (interpreter) buduje zsc natywnie (C)  -> zsc2
#   stage3: zsc2 buduje zsc natywnie                      -> zsc3
#   stage4: zsc3 buduje zsc natywnie                      -> zsc4
# Warunek: C wygenerowany przez interpreter, zsc2, zsc3 i zsc4 dla zsc jest identyczny (punkt stały), oraz
# zsc3 generuje identyczny C jak interpreter dla wszystkich testów natywnych.
set -euo pipefail
. "$(dirname "$0")/lib.sh"
[ -n "$ZSC_DIR" ] || { echo "stage3: brak repo zsc (ustaw ZSC_DIR)"; exit 2; }
ensure_stage0; link_workspace; trap unlink_workspace EXIT
T="$(mktemp -d)"; trap 'rm -rf "$T"; unlink_workspace' EXIT
MAIN="$ZSC_DIR/src/main.zsharp"
echo "== stage2: interpreter -> zsc natywny"
"$ZSC0" run "$MAIN" build "$MAIN" -o "$T/zsc2" >/dev/null
"$ZSC0" run "$MAIN" build "$MAIN" --emit c -o "$T/ref.c" >/dev/null
echo "== stage3/4: zsc natywny buduje siebie"
"$T/zsc2" build "$MAIN" -o "$T/zsc3" >/dev/null
"$T/zsc3" build "$MAIN" -o "$T/zsc4" >/dev/null
for s in zsc2 zsc3 zsc4; do
  "$T/$s" build "$MAIN" --emit c -o "$T/$s.c" >/dev/null
  cmp -s "$T/ref.c" "$T/$s.c" || { echo "BŁĄD: C z $s różni się od C z interpretera"; exit 1; }
done
echo "   punkt stały: C identyczny ($(wc -l < "$T/ref.c") linii) dla interpretera, zsc2, zsc3, zsc4"
echo "== zsc3 vs interpreter na testach natywnych"
n=0; for f in "$ZSC_DIR"/tests/native/*.zsharp; do
  "$ZSC0" run "$MAIN" build "$f" --emit c -o "$T/a.c" >/dev/null 2>&1 || true
  "$T/zsc3" build "$f" --emit c -o "$T/b.c" >/dev/null 2>&1 || true
  cmp -s "$T/a.c" "$T/b.c" || { echo "BŁĄD: różnica dla $f"; exit 1; }; n=$((n+1))
done
echo "   $n programów: identyczny C"
echo "stage3: OK"
