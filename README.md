# Z-Sharp (stage0 + biblioteki wspólne)

Rdzeń bootstrapu Z#: **stage0** (Rust) + biblioteki Z# współdzielone przez `zsc`, `zsi`, `zst`.

```
cargo build --release
./target/release/zsc0 run examples/hello.zs
cargo test                      # lekser, parser, testy złote
tests/lexer_parity.sh lib/lexer.zs examples/hello.zs   # lekser Rust == lekser Z#
```

## Zawartość
- `crates/zs-lexer`, `crates/zs-parser` (Pratt, pełne AST), `crates/zs-interp` (interpreter AST), `stage0/` (`zsc0 run|check|--tokens|--ast`)
- `lib/lexer.zs` — samohostowany lekser Z# (parytet pozycji z Rustem), `lib/cli.zs` — parser argumentów
- `zworkspace.json` — mapa członków dla `import Workspace.<nazwa>`; repozytoria zsc/zsi/zst dołączają to repo jako submoduł `Z-Sharp/`

## Decyzje językowe podjęte w stage0 (spec 0.3 była niejednoznaczna)
- `.` po **gołej nazwie** (`System.cpuCount()`, `Dog.KIND`, `Dog.count`) = moduł/klasa; w pozostałych miejscach `.` = konkatenacja.
- Obiekty: `->`. Statyczne właściwości bez `$`: `Dog.count`.
- `func` i `function` równoważne. `echo` bez nowej linii; `println` dodaje.
- `==` numeryczne dla liczb, reszta jak `===` (brak magii PHP). `/` daje int gdy dzieli się bez reszty.
- Zmienne funkcji są lokalne; domknięcia przechwytują migawkę zmiennych w chwili utworzenia.
- Typy parametrów/zwrotów sprawdzane w runtime (int→float koercja). Widoczność (`private`) parsowana, **nie egzekwowana**.
- `extern "lib" { func f(T $x): R; }` — składnia (propozycja) parsowana; linkowanie dopiero w zsc.

## Ograniczenia stage0 (uczciwie)
- To interpreter drzewa AST z `Rc`, nie ZAMM i nie kompilator natywny. Szybkość: rząd wielkości wolniej niż docelowy VM.
- Brak: interpolacji stringów, traitów, generyków, prawdziwej asynchroniczności (`Task.run` jest synchroniczne), pakietów/namespace poza prostym `import`.
- Stdlib: minimalna (System, Math, File, Directory, Process, Task + funkcje tekstu/tablic).
