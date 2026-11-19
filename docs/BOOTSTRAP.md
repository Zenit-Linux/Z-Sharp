# Bootstrap Z# (0.0.1)

| Etap | Co | Stan |
|---|---|---|
| **stage0** | interpreter AST w Rust (`zsc0`): `run` (źródła) i `exec` (pakiet `.zbc`) | działa |
| **stage1-front** | lekser + parser w Z# (`lib/lexer.zsharp`, `lib/parser.zsharp`) | działa; parytet z Rust (`tests/*_parity.sh`), w tym samoparsowanie |
| **stage1-sem** | analiza semantyczna + analiza ucieczki (`lib/sema.zsharp`) → klasy ZAMM | działa; 0 ostrzeżeń na własnych źródłach |
| **stage1-pack** | `.zbc` (`lib/zbc.zsharp`): program bez źródeł, deterministyczny | działa |
| **punkt stały** | `zsc` (z `.zbc`) kompiluje samego siebie: s1 == s2 == s3 | **działa** (`scripts/fixpoint.sh`) |
| **stage1-core** | ZAMM (`zamm/*.zsharp`) i runtime (`runtime/main.zsharp`) napisane w Z#, 1:1 z referencją w Rust | działa; `tests/zamm_test.zsharp` (39 asercji), `tests/runtime_test.zsharp` (16) |
| **stage1-ir** | Z# IR strukturalne (`lib/ir.zsharp`): lowering, walidacja, lokalne, `--emit ir` | działa (SSA i bytecode VM: do zrobienia) |
| **stage2** | backend C (`lib/emit_c.zsharp`) → `cc` + `libzs_runtime.a`: `zsc build` daje binarkę natywną | działa dla podzbioru; testy różnicowe interpreter↔natywny (`zsc/tests/native/run_diff.sh`) |
| stage2-llvm | backend LLVM | do zrobienia |
| stage3 | brak zależności od Rusta jako wykonawcy: binarka `zsc` kompiluje `zsc` | cel — blokuje go brak klas/domknięć/map w backendzie natywnym |

## Co dokładnie dowodzi punkt stały
* s1: `zsc` uruchomiony **ze źródeł** (parsowanych przez Rust) pakuje własne źródła.
* s2: `zsc` uruchomiony **z s1.zbc** — kompilator wykonuje się bez parsowania własnych źródeł w Rust — pakuje te same źródła.
* s3: to samo z s2.zbc. Warunek: s1 == s2 == s3 bajt w bajt.

To jest bootstrap **frontendu i pakera** (stage1). Wykonawcą nadal jest Rust (`zsc0 exec`), tak jak VM dla Lua/Python.
Pełny bootstrap (stage3) wymaga backendu, który emituje kod niezależny od stage0 — to następny krok: Z# IR, bytecode/VM lub LLVM.

## Format `.zbc`
Tekst: `ZBC 1`, potem sekcje `@@module <ścieżka importu>` i `@@entry`, jedna instrukcja AST (S-expression, `docs/AST.md`) na linię.
Ograniczenie: pakowane są importy najwyższego poziomu pliku; klucz modułu = ścieżka importu zapisana w kodzie (konflikt → błąd).

## `main` nie jest wymagane
Program może składać się z samego kodu najwyższego poziomu. Jeśli zdefiniowano `func main` i nie wywołano go jawnie,
runtime wywołuje je po kodzie najwyższego poziomu; zwrócona liczba całkowita to kod wyjścia procesu.

## Weryfikacja
`scripts/bootstrap.sh`: build, testy Rust (manifest, ZAMM, runtime, czytnik AST), test złoty, `nomain`, parytet leksera/parsera,
sema (złoty + 0 ostrzeżeń), manifesty, narzędzia, punkt stały, `zsi` na `.zbc`, ABI C.

## Bootstrap rdzenia (lib + zamm + runtime)
| człon | w Z# | ziarno w Rust | weryfikacja |
|---|---|---|---|
| lexer, parser | `lib/lexer.zsharp`, `lib/parser.zsharp` | `stage0/zs-lexer`, `zs-parser` | parytet pozycji tokenów i AST (`tests/*_parity.sh`) |
| manifest (`Z#.toml`) | `lib/manifest.zsharp` | `stage0/zs-manifest` | `tests/manifest_test.zsharp` + parytet wyjścia `zsc0 manifest` (`tests/manifest_parity.sh`) |
| sema, zbc, ir, emit_c | `lib/*.zsharp` | — (tylko Z#) | golden sema, punkt stały, testy różnicowe |
| ZAMM | `zamm/*.zsharp` | `stage0/zamm` | `tests/zamm_test.zsharp` odpowiada `zamm/tests/zamm.rs` |
| runtime | `runtime/main.zsharp` | `stage0/runtime` | `tests/runtime_test.zsharp` odpowiada `runtime/tests/abi.rs` |

Model pamięci w Z#: stage0 nie ma surowej pamięci, więc ZAMM w Z# działa na **symulowanej przestrzeni adresowej** (`zamm/chunk.zsharp`:
adresy chunków, rzadka mapa komórek, poison `0xDD`). Cała logika (przesuwanie wskaźnika, wyrównanie, rewind, pula, LOS, generacje uchwytów,
`pop_frame_returning`) jest prawdziwym kodem Z#; różnica względem Rusta to brak bajtów (komórka = wartość; napisy w komórkach to znaki, rozmiar w bajtach UTF-8).
`scripts/core_status.sh` pokazuje, ile każdego członu jest w Z#, a ile w ziarnie.

## Backend natywny (zsc)
`zsc build x.zs` → AST → sema → IR → C (`emit_c`) → `cc` + `libzs_runtime.a`. Kolejność ewaluacji lewa→prawa, tablice mają semantykę wartości,
komunikaty błędów i numery linii są takie jak w stage0 (testowane różnicowo). Konstrukcje spoza podzbioru kończą się błędem `E0101…E0107`. Wyjątki: `setjmp/longjmp`, lokalne `volatile` w funkcjach z `try`, hierarchia jak w stage0 (`Error extends Exception`).
