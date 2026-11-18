# Z-Sharp 0.0.1 (stage0 + runtime + ZAMM + biblioteki wspólne)

Rdzeń bootstrapu Z#. Repozytoria `zsc`, `zsi`, `zst`, `zzc` dołączają to repo jako `Z-Sharp/` i importują je przez `import Workspace.<człon>;`.

```
scripts/bootstrap.sh          # buduje stage0, testuje wszystko: parytet stage1, sema, punkt stały zsc
scripts/fixpoint.sh           # zsc kompiluje sam siebie: s1 == s2 == s3
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
| `lib/` | `lexer.zsharp`, **`parser.zsharp`** (stage1, parytet z Rust), `cli.zsharp`, **`sema.zsharp`** (analiza ucieczki → ZAMM), **`zbc.zsharp`** (pakiet `.zbc`) |
| `zamm/` | **ZAMM**: chained arenas, pula chunków, LOS, uchwyty z generacją, zakresy/zadania, `SharedSpace` (`zamm/README.md`) |
| `runtime/` | **`libzs_runtime.a`**: `ZValue`, napisy, tablice, zasoby, statystyki; ABI C w `runtime/include/zs_runtime.h` |
| `tests/` | `basics`, parytet leksera/parsera (`*_parity.sh`), przypadki błędów `bad/` |
| `docs/` | `SPEC.md` (0.4), `AST.md`, `BOOTSTRAP.md` |

## Decyzje językowe stage0
- `.` po **gołej nazwie** (`System.cpuCount()`, `Dog.KIND`) = moduł/klasa; gdzie indziej `.` = konkatenacja. Obiekty: `->`.
- `func` i `function` równoważne. `echo` bez nowej linii, `println` z nową linią.
- `==` numeryczne dla liczb, reszta jak `===`. `/` daje int, gdy dzieli się bez reszty.
- Typy parametrów/zwrotów sprawdzane w runtime (int→float koercja). Widoczność parsowana, **nie egzekwowana**.
- `extern "lib" { func f(T $x): R; }` — składnia parsowana; linkowanie dopiero w zsc.
- **`main` nie jest wymagane**: jeśli jest zdefiniowane i nie wywołane, uruchamia się po kodzie najwyższego poziomu (zwrócony int = kod wyjścia).
- Rozszerzenia źródeł: `.zsharp` (kanoniczne w repozytoriach), `.zs` (zgodność ze specyfikacją) — resolver próbuje obu.

## Ograniczenia (uczciwie)
- stage0 to interpreter drzewa AST na `Rc`; **sam nie używa ZAMM**. ZAMM/runtime to biblioteka natywna dla przyszłego backendu (zsc) i VM (zsi).
- Brak Z# IR, bytecode VM/JIT i backendu LLVM — to kolejne etapy; pełny bootstrap (bez Rusta jako wykonawcy) jeszcze nie jest osiągnięty (`docs/BOOTSTRAP.md`).
- Brak: interpolacji stringów, traitów, generyków, prawdziwej asynchroniczności, mapy/obiektów w runtime natywnym.
