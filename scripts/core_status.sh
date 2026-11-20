#!/usr/bin/env bash
# Rozliczenie rdzenia: ile kodu jest w Z#, a ile w ziarnie (Rust stage0) i w runtime C.
. "$(dirname "$0")/lib.sh"
loc() { cat "$@" 2>/dev/null | grep -cv '^\s*$'; }
printf "%-34s %8s\n" "człon" "linii"
for f in lexer parser sema ir emit_c emit_llvm zbc manifest cli; do printf "%-34s %8s\n" "lib/$f.zsharp (Z#)" "$(loc "$ROOT/lib/$f.zsharp")"; done
for f in "$ROOT"/lib/bytecode.zsharp "$ROOT"/lib/vm.zsharp; do [ -f "$f" ] && printf "%-34s %8s\n" "lib/$(basename "$f") (Z#)" "$(loc "$f")"; done
printf "%-34s %8s\n" "wrapper/main.zsharp (Z#)" "$(loc "$ROOT/wrapper/main.zsharp")"
printf "%-34s %8s\n" "zamm + runtime w Z#" "$(loc "$ROOT"/zamm/*.zsharp "$ROOT"/runtime/*.zsharp)"
printf "%-34s %8s\n" "runtime/native (C)" "$(loc "$ROOT"/runtime/native/*.c "$ROOT"/runtime/include/*.h)"
printf "%-34s %8s\n" "stage0 (Rust, ziarno)" "$(find "$ROOT/stage0" -name '*.rs' -not -path '*/target/*' | xargs cat | grep -cv '^\s*$')"
Z=$(cat "$ROOT"/lib/*.zsharp | grep -cv '^\s*$'); R=$(find "$ROOT/stage0" -name '*.rs' -not -path '*/target/*' | xargs cat | grep -cv '^\s*$')
echo "frontend+backendy w Z#: $Z linii; ziarno Rust: $R linii"
