#!/usr/bin/env bash
# Punkt stały: zsc (jako .zbc) kompiluje sam siebie. s1 == s2 == s3 (bajt w bajt), potem pakiet standalone.
set -euo pipefail
. "$(dirname "$0")/lib.sh"; ensure_stage0
[ -n "$ZSC_DIR" ] || { echo "fixpoint: nie znaleziono repo zsc (ustaw ZSC_DIR)"; exit 1; }
link_workspace; trap unlink_workspace EXIT
T="$(mktemp -d)"; trap 'unlink_workspace; rm -rf "$T"' EXIT
SRC="$ZSC_DIR/src/main.zsharp"
"$ZSC0" run "$SRC" build "$SRC" --emit zbc -o "$T/s1.zbc" >/dev/null     # stage0 (Rust) -> s1
"$ZSC0" exec "$T/s1.zbc" build "$SRC" --emit zbc -o "$T/s2.zbc" >/dev/null # s1 -> s2
"$ZSC0" exec "$T/s2.zbc" build "$SRC" --emit zbc -o "$T/s3.zbc" >/dev/null # s2 -> s3
cmp "$T/s1.zbc" "$T/s2.zbc" && cmp "$T/s2.zbc" "$T/s3.zbc" && echo "fixpoint: s1 == s2 == s3 ($(wc -c <"$T/s3.zbc") B)"
# samodzielna binarka zsc (runtime + zbc), bez Rusta w ścieżce użytkownika
"$ZSC0" bundle "$T/s3.zbc" -o "$T/zsc" >/dev/null
echo 'echo "fixpoint-ok", 40 + 2;' > "$T/p.zs"
"$T/zsc" build "$T/p.zs" -o "$T/p" >/dev/null && [ "$("$T/p")" = "fixpoint-ok 42" ] && echo "standalone zsc: buduje i uruchamia program natywny"
