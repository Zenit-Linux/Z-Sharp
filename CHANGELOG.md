# Zmiany (wersja pozostaje 0.0.1)

## Sesja „zenitd jako poligon: extern w stage0"
- **stage0**: wywołania `extern "c"` w interpreterze (`zs-interp/src/ffi.rs`): dlsym + do 6 argumentów całkowitych; `string` ↔ `char *` (null ↔ NULL). `zsc0 run`, `exec` i bundle uruchamiają teraz kod z libc (dotąd: błąd „linkowanie extern tylko w zsc").
- **stage0**: `zsc0 bundle <pakiet.zbc> -o <wyjście>` — brakowało go w `main.rs`, mimo że `scripts/fixpoint.zsharp` go wołał (etap „standalone zsc" kończył się błędem).
- **stage0**: wątek główny blokuje sygnały, więc `SIGCHLD` skierowany do procesu trafia do `signalfd` w wątku interpretera (wcześniej ginął; demony na `signalfd` widziały zakończenie dzieci dopiero po przeterminowaniu pollingu).
- **stage0**: bundle jako PID 1 — przed zamontowaniem `/proc` `current_exe` zawodzi, więc ładunek jest szukany przez `argv[0]`; wątek interpretera próbuje coraz mniejszych stosów (1 GiB → 4 MiB), gdy rezerwacja się nie udaje (VM z małą pamięcią).
- Test złoty `tests/m3/extern.zsharp`.

## Sesja „rozbudowa stdlib"
- Nowe moduły `Std.*` (czysty Z#, identyczne w interpreterze i w kodzie natywnym C): **Base64** (+hex), **Hash** (SHA-256/SHA-1/MD5/HMAC/CRC-32/Adler-32/FNV-1a),
  **Csv** (RFC 4180), **Random** (`Rng`, xorshift128, `uuid4`), **Text** (`format` z interpolacją `{klucz}`, `slug`, `wrap`, `levenshtein`, …). Opis: `docs/STDLIB.md`.
- Testy złote `tests/std/{base64,hash,csv,random,text}` (skróty zweryfikowane względem Pythona); zarejestrowane w `stage0/zs-interp/src/stdlib.rs`.
- Naprawiony nieaktualny test `tests/std/net_client` (oczekiwał braku HTTPS; od czasu transportu przez `curl` sprawdza tylko prefiks błędu).
- `run_all`: 27 grup OK w interpreterze; `--native`: nowe moduły OK, `tests/zamm_test.zsharp [natywny C]` nadal nie przechodzi (znany brak klas w tym teście).
- Uwaga środowiskowa: `Cargo.lock` w wersji 4 wymaga cargo ≥ 1.78; z cargo 1.75 usuń plik i zbuduj ponownie.

## Sesja „ZAMM i M6"
- **M4 / ZAMM**: konserwatywny GC usunięty; runtime natywny używa aren per ramka (`zs_enter`/`zs_leave`), promocji ucieczek (return, bariera zapisu, statyki, wyjątki), regionów pętli z analizą żywotności (`lib/regions.zsharp`). Opis: `docs/MEMORY.md`. Testy: `tests/m4/zamm`, `tests/m4/gc_stress`. stage3 nadal daje identyczny C.
- **M6**: VM bajtkodowa w stage0 (`vm.rs`, test różnicowy `tests/m6/vm`), `zsi --jit` z cache i fallbackiem. Opis: `docs/VM.md`.
- **M3 (luki)**: wyrażenie `match` w backendzie natywnym (`lib/matchlit.zsharp`); wywołania funkcji wariadycznych z pominiętymi parametrami domyślnymi; `??` na zagnieżdżonych brakujących kluczach w runtime natywnym.

## Sesja „M3/M4/M5 domknięte, strona WWW"
- **M4 (etap pośredni, zastąpiony ZAMM)**: konserwatywny GC mark-and-sweep w `runtime/native/zs_rt.c` (klasy rozmiarów, chunki 64 KiB, wskaźniki wewnętrzne, korzenie: stos/rejestry/.data+.bss),
  `System.gc()`, `System.gcStats()`, zmienne `ZS_GC*`, tryb `ZS_GC_STRESS`. Opis i odstępstwo od SPEC: `docs/MEMORY.md`. Test: `tests/m4/gc_stress`. stage3 nadal daje identyczny C.
- **M3**: spread w literałach tablic `[...$a]` (Rust: `ExprKind::Spread`; Z#: parser, sema, nowy `lib/spreadlit.zsharp` w potoku po `link`). Test: `tests/m3/spread`.
- **M5**: strefy czasowe w `Std.Time` (`inZone`, `makeInZone`, `addInZone`, `zoneInfo`, `%Z`), regex: lookbehind, `\p{..}`/`\P{..}`, flaga `u`; HTTPS w `Std.Net` przez `curl`.
  Testy: `tests/std/time_zones`, `tests/std/regex_unicode`.
- stage0: stdlib i prymitywy systemowe w `zs-interp/src/{stdlib,sys}.rs`, `System.gc/gcStats` (atrapy).
- Strona `website/` (TypeScript + esbuild, testy `node:test`), `index.html` w korzeniu, workflow `build-docs.yml` (GitHub Pages), `check.yml` (CI).
- Znane braki: domknięcia w zst js, natywny TLS, pełne tablice Unicode, `Socket` w `builtin_modules()` linkera (Std.Net natywnie nietestowany).

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
