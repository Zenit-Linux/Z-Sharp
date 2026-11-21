# Zmiany (wersja pozostaje 0.0.1)

## Sesja „M3 domknięty, M5 stdlib, skrypty w Z#"
- **M3 (domknięcia i nie tylko) w backendach natywnych**: `lib/closures.zsharp` (lifting domknięć z przechwyceniem przez wartość, funkcje strzałkowe,
  wywołania wartości `$f(...)`, `zs_call`/`zs_spread`), argumenty nazwane i spread także dla metod (`zs_invoke_n`), pola statyczne (`zs_sget/zs_sset`),
  funkcje wyższego rzędu `map/filter/reduce/each/find/any/all/sum/usort` (`lib/prelude.zsharp` w Z# dla C/LLVM; wbudowane w stage0). Testy: `tests/m3/`.
- **M5 — biblioteka standardowa** (`stdlib/`, `docs/STDLIB.md`): `Std.Json`, `Std.Toml`, `Std.Regex`, `Std.Time`, `Std.Path`, `Std.Os`, `Std.Net`
  (TCP/UDP/DNS, URL, klient i serwer HTTP/1.1; bez TLS). Działają identycznie w interpreterze, C i LLVM. Testy: `tests/std/`.
- Nowe prymitywy w stage0 i runtime C: `System.{setEnv,unsetEnv,environ,cwd,chdir,pid,hostname,nowNanos,monotonic,sleep,localOffset,bytes,fromBytes}`,
  `File.{stat,rename,realpath,chmod,symlink,readlink}`, `Directory.remove`, moduł `Socket`. Deklaracje LLVM generowane z `IrTables.MODULES`.
- stage0: `zsc0 bundle`, import `Std.X` (stdlib wkompilowana + cache), poprawiony alias runtime w `stage0/Cargo.toml`.
- **Skrypty w Z# (shebang `#!/usr/bin/env zsi`)**: `scripts/{bootstrap,core_status,fixpoint,stage3,lib}.zsharp` zastępują `.sh`;
  `scripts/tests/{lexer_parity,parser_parity,run_all}.zsharp` — prawdziwy Z# (poprzednio bash/pusty plik). Wspólna biblioteka: `import Workspace.Scripts;`.
- `scripts/Linux-Distributions/{install,remove}.sh`: instalacja/usuwanie `zsi zzc zsc zst` na dowolnej dystrybucji; wersja z `Z#.toml` narzędzia,
  binarka z `releases/download/<wersja>/<narzędzie>`; `--immutable` (także `--immuntable`) → `$HOME/.local/bin`.
- Workflowy `build.yml` w zsc/zsi/zst/zzc: instalator Z# + budowa ze źródeł + test dymny + artefakt/asset wydania. `zzc/Z#.toml` uzupełniony;
  zależności narzędzi wskazują `Zenit-Linux/Z-Sharp`.

## Sesja „klasy natywnie i stage3"
- `lib/classes.zsharp`: lowering klas do funkcji (obiekty jako uchwyty `ZS_OBJ` z tablicą pól; `new` → `Klasa__new`, metody → `Klasa__m($this,…)`, dyspozytor `zs_invoke`, `zs_isa`, stałe wstawiane, `parent.`/`Klasa.statyczna()` statycznie); klasy wyjątków (`extends Exception/Error`, catch → łańcuch `zs_isa`); zapis `$o->pole[...] = v` bez kopiowania tablicy (`zs_obj_take/put`).
- Wariadyczne `...$x` (pakowanie argumentów), `System.version`, `System.resolveImport` w runtime C.
- Runtime C: obiekty, rzucanie obiektów-wyjątków, komunikaty zgodne ze stage0; runtime JS w zst: obiekty.
- `throw` ustawia numer linii po ewaluacji wyrażenia (zgodnie z interpreterem).
- `scripts/stage3.sh` (w `bootstrap.sh`): natywna samokompilacja zsc, C identyczny dla interpretera/zsc2/zsc3/zsc4.
- `zsc/src/main.zsharp`: brakujący `import Workspace.emit_c`.

## Sesja „moduły i bity"
- `lib/link.zsharp`: natywny linker modułów Z# (funkcje + import + namespace; cykle i kod top-level w module → błąd `E0106`); używany przez `zsc` i `zst`.
- Operatory bitowe (`& | ^ << >> ~`) w runtime C (`zv_bit*`, `zv_shl/shr`, maskowanie przesunięć jak stage0), backendach C/LLVM i w `zst` js.
- Testy: `imports`, `bitops`, `bitops64` (natywne), `imports`/`bitops`/`mod_*` (zst).

## Sesja „niezależność"
- **Natywny runtime w C**: `runtime/include/zs_native.h` + `runtime/native/zs_rt.c` (wartości, tablice uporządkowane z haszem i COW, napisy UTF-8, wyjątki setjmp, ~60 wbudowanych, moduły Math/System/File/Directory/Process). Archiwum nie zawierało nagłówka, który emituje `emit_c` — odtworzony od zera; backend nie wymaga już `libzs_runtime.a`.
- **Backend LLVM**: `lib/emit_llvm.zsharp` (`zsc build --backend llvm`, `--emit ll`, `--target <triple>`); `z#`/`zsc`/`zzc` korzystają z tego samego runtime.
- `emit_c`: moduły `File/Directory/Process/System.*` (`m_<Moduł>_<fn>`), `catch (Error)` łapie tylko `Error`, poprawny zapis zagnieżdżonych tablic (write-back), `main(argc, argv)`, `push`/`dump` wieloargumentowe, `pad_*`/`array_slice` 2–3 arg., `utf8_lossy` w `substr` (jak stage0).
- **stage0**: `zsc0 bundle <pakiet.zbc> -o <wyjście>` — binarka standalone (runtime + ładunek). Naprawiony alias `zs-runtime` w `stage0/Cargo.toml` (archiwum się nie budowało).
- `scripts/{bootstrap,fixpoint,core_status,lib}.sh`, `tests/{run_all,lexer_parity,parser_parity}.sh` — brakowały w archiwum.
- **wrapper/**: źródło binarki `z#` (new/init/build/run/check/test/transpile/clean/doctor/stages), 34 testy.
- Testy różnicowe natywne: `zsc/tests/native` (C i LLVM vs interpreter).

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
