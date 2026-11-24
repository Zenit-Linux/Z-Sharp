# Z-Sharp 0.0.1 (stage0 seed + rdzeń w Z#: lib + ZAMM + runtime)

Rdzeń bootstrapu Z#. Repozytoria `zsc`, `zsi`, `zst`, `zzc` dołączają to repo jako `Z-Sharp/` i importują je przez `import Workspace.<człon>;`.

```
scripts/bootstrap.zsharp      # buduje stage0, testuje wszystko: parytet stage1, sema, punkt stały zsc
scripts/fixpoint.zsharp       # zsc kompiluje sam siebie: s1 == s2 == s3
scripts/core_status.zsharp    # ile rdzenia jest już w Z#, a co jest ziarnem w Rust
scripts/tests/run_all.zsharp # testy złote (rdzeń, m3, stdlib, sieć) + parytety lexera i parsera (--native: też backend C)
cargo build --release         # target/release/zsc0 (stage0) + libzs_runtime.a
./target/release/zsc0 run examples/hello.zsharp     # źródło
./target/release/zsc0 exec plik.zbc                  # pakiet .zbc (bez parsowania źródeł)
./target/release/zsc0 manifest .        # walidacja Z#.toml i członków workspace'u
./target/release/zsc0 --sexp plik       # AST jako S-expression (docs/AST.md)
```

## Układ
| Ścieżka | Zawartość |
|---|---|
| `Cargo.toml` | korzeń workspace'u Cargo (stage0, zamm, runtime) |
| `Z#.toml` | manifest Z# (zastępuje `zproject.json`/`zworkspace.json`): `[package]`, `[workspace.members]`, `[dependencies]`, `[extern.*]`, `[memory]` |
| `stage0/` | `zs-lexer`, `zs-parser` (+ `dump`), `zs-interp`, **`zs-manifest`** (parser TOML + model manifestu + resolver importów), CLI `zsc0` |
| `lib/` | `lexer.zsharp`, `parser.zsharp` (parytet z Rust), `cli.zsharp`, `sema.zsharp` (analiza ucieczki → ZAMM), `zbc.zsharp` (pakiet `.zbc`), **`ir.zsharp`** (Z# IR + walidacja), **`emit_c.zsharp`** (backend C), **`manifest.zsharp`** (parser `Z#.toml`, parytet z `zs-manifest`) |
| `zamm/` | **ZAMM w Z#** (`*.zsharp`: config, chunk, arena, heap, shared) — chained arenas, pula chunków, LOS, uchwyty z generacją, zakresy/zadania, przestrzeń współdzielona; testy `tests/zamm_test.zsharp` (`zamm/README.md`) |
| `runtime/` | **runtime w Z#** (`main.zsharp`: `ZValue`, napisy, tablice, zasoby, statystyki) + nagłówki C: `include/zs_runtime.h` (ABI `libzs_runtime.a`), `include/zs_native.h` (operacje dla kodu generowanego przez zsc) |
| `wrapper/` | źródło binarki `z#` (wrapper nad zsc/zsi/zst/zzc) |
| `runtime/native/`, `runtime/include/` | natywny runtime w C (`zs_rt.c`, `zs_native.h`) dla backendów C i LLVM |
| `tests/` | `basics`, `nomain`, sema, `zamm_test`, `runtime_test`, parytet leksera/parsera (`scripts/tests/*_parity.zsharp`), `bad/`, `m3/`, `std/`, `scripts/tests/run_all.zsharp` |
| `stdlib/` | biblioteka standardowa `Std.*` (json, toml, regex, time, path, os, net, base64, hash, csv, random, text) — `docs/STDLIB.md` |
| `scripts/` | skrypty w Z# (`bootstrap`, `fixpoint`, `stage3`, `core_status`, `lib`, `tests/`) oraz `Linux-Distributions/{install,remove}.sh` |
| `docs/` | `SPEC.md` (0.4), `AST.md`, `BOOTSTRAP.md`, `STDLIB.md` |

## Decyzje językowe stage0
- `.` po **gołej nazwie** (`System.cpuCount()`, `Dog.KIND`) = moduł/klasa; gdzie indziej `.` = konkatenacja. Obiekty: `->`.
- `func` i `function` równoważne. **`echo a, b;`** wypisuje argumenty rozdzielone spacją i kończy nową linią; `necho(a, b)` — bez separatora i bez nowej linii; `eecho(a, b)` — jak `echo`, na stderr.
- `==` numeryczne dla liczb, reszta jak `===`. `/` daje int, gdy dzieli się bez reszty.
- Typy parametrów/zwrotów sprawdzane w runtime (int→float koercja). Widoczność parsowana, **nie egzekwowana**.
- `extern "lib" { func f(T $x): R; }` — składnia parsowana; linkowanie dopiero w zsc.
- **`main` nie jest wymagane**: jeśli jest zdefiniowane i nie wywołane, uruchamia się po kodzie najwyższego poziomu (zwrócony int = kod wyjścia).
- Rozszerzenia źródeł: `.zsharp` (kanoniczne w repozytoriach), `.zs` (zgodność ze specyfikacją) — resolver próbuje obu.

## Ograniczenia (uczciwie)
- stage0 (Rust) to nadal **ziarno**: uruchamia Z# (`zsc0`) i dostarcza `libzs_runtime.a` linkowane z natywnymi binarkami. Definicje lexera, parsera, sema, IR, backendu C, ZAMM i runtime są już w Z#, a ZAMM i runtime w Z# mają testy zgodności z odpowiednikami Rusta. Same nie są jednak jeszcze wykonywalne natywnie (backend natywny nie obsługuje klas/domknięć).
- Backend natywny `zsc` (C → cc) obsługuje podzbiór: funkcje, tablice (indeksowane i asocjacyjne, wyszukiwanie liniowe), napisy, int/float/bool, pętle, `switch`, `try/catch/finally/throw` (Exception, Error, Throwable), podstawowe funkcje wbudowane. Reszta (klasy, domknięcia, moduły) daje jawny błąd kompilacji z kodem (`E01xx`). Pełny język: `zsc build --emit zbc` + `zsi`.
- Pamięć w binarkach natywnych: wszystko w czasie życia `Persistent` (bezpieczne, bez zwalniania do końca procesu). Obniżanie do ramek wg analizy ucieczki (`sema`) — do zrobienia.
- Brak: bytecode VM/JIT, backendu LLVM, SSA, interpolacji stringów, traitów, generyków, prawdziwej asynchroniczności; obiekty/klasy w runtime natywnym.
- Stan etapów i rozliczenie rdzenia: `docs/BOOTSTRAP.md`, `scripts/core_status.zsharp`.
