# Zmiany (wersja pozostaje 0.0.1)

- Rdzeń w Z#: **`lib/manifest.zsharp`** (parser `Z#.toml`: wartości, tabele, `[[tablice]]`, inline, klucze kropkowane, walidacja, `find_manifest`, raport) z parytetem względem `zsc0 manifest`; `[memory]` → `zamm.config_from`.
- Backend natywny: **tablice asocjacyjne** (klucze int/string, kolejność wstawiania, `keys/values/has_key`), **wyjątki** (`try/catch/finally`, `throw new Exception/Error`, `getMessage`, `break/continue/return` przez `finally`). Testy różnicowe: 16 programów.

- `print` → **`necho`** (echo bez nowej linii i bez separatora, wiele argumentów); `echo` z nową linią, `eecho` na stderr.

- `println` → **`echo`** wszędzie (interpreter, sema, narzędzia, testy, dokumentacja). `echo` dodaje spacje między argumentami i nową linię; `eprintln` → `eecho`; `print` bez zmian (bez nowej linii).
- ZAMM i runtime przepisane do Z# (`zamm/*.zsharp`, `runtime/main.zsharp`) z testami zgodności.
- Nowe: `lib/ir.zsharp`, `lib/emit_c.zsharp`, `runtime/include/zs_native.h`, `zs_array_set` w `libzs_runtime.a`.
- `zsc`: `build` (exe/c/ir/zbc), `run`, `check --strict/--native`, `analyze`, `stages`, kody wyjścia, diagnostyki `plik:linia: błąd [Exxxx]`.
- `zst`: `echo`/`eecho` transpilowane do pomocników PHP zachowujących semantykę Z#.
- Odtworzone brakujące elementy archiwum: korzeń `Cargo.toml`, `scripts/{bootstrap,fixpoint,core_status}.sh`, `tests/{run_all,lexer_parity,parser_parity}.sh`, `zamm/README.md`.
- Naprawione: alias zależności `zs-runtime` w `stage0/Cargo.toml`; `tests/sema_sample.zsharp` (nieparsowalny `echo(a, b)`); `tests/basics.zsharp`/`nomain` zgodne z `.out`.
