#!/usr/bin/env bash
# Pełna weryfikacja łańcucha bootstrapu: stage0 -> testy rdzenia -> punkt stały zsc -> testy natywne (C i LLVM) -> wrapper.
set -euo pipefail
. "$(dirname "$0")/lib.sh"
echo "== stage0 (ziarno Rust)"; (cd "$ROOT" && cargo build --release --quiet) && echo "   $ZSC0"
echo "== testy rdzenia"; "$ROOT/tests/run_all.sh"
if [ -n "$ZSC_DIR" ]; then
  echo "== punkt stały zsc"; "$ROOT/scripts/fixpoint.sh"
  link_workspace; trap unlink_workspace EXIT
  echo "== backend natywny (C + LLVM) vs interpreter"
  [ -x "$ZSC_DIR/tests/native/run_diff.sh" ] && ZS_HOME="$ROOT" "$ZSC_DIR/tests/native/run_diff.sh"
  echo "== stage3 (natywna samokompilacja zsc)"; "$ROOT/scripts/stage3.sh"
  echo "== wrapper z#"
  T="$(mktemp -d)"; "$ZSC0" run "$ZSC_DIR/src/main.zsharp" build "$ROOT/wrapper/main.zsharp" -o "$T/z#" >/dev/null && "$T/z#" version
  rm -rf "$T"
fi
"$ROOT/scripts/core_status.sh"
