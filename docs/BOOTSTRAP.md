# Bootstrap Z# (0.0.1)

| Etap | Co | Stan |
|---|---|---|
| **stage0** | interpreter AST w Rust (`zsc0`): `run` (źródła) i `exec` (pakiet `.zbc`) | działa |
| **stage1-front** | lekser + parser w Z# (`lib/lexer.zsharp`, `lib/parser.zsharp`) | działa; parytet z Rust (`tests/*_parity.sh`), w tym samoparsowanie |
| **stage1-sem** | analiza semantyczna + analiza ucieczki (`lib/sema.zsharp`) → klasy ZAMM | działa; 0 ostrzeżeń na własnych źródłach |
| **stage1-pack** | `.zbc` (`lib/zbc.zsharp`): program bez źródeł, deterministyczny | działa |
| **punkt stały** | `zsc` (z `.zbc`) kompiluje samego siebie: s1 == s2 == s3 | **działa** (`scripts/fixpoint.sh`) |
| stage1-ir | Z# IR (SSA) + bytecode, VM natywna (`zsi`) | do zrobienia |
| stage2 | backend natywny (LLVM; C jako etap przejściowy) linkujący `libzs_runtime.a` | do zrobienia |
| stage3 | brak zależności od Rusta jako wykonawcy: binarka `zsc` kompiluje `zsc` | cel |

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
