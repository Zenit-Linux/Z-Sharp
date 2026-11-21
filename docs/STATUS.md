# Status Z# i mapa drogowa (stan: sesja „niezależność")

Legenda: ✅ działa i jest testowane · 🟡 częściowo (podzbiór/ograniczenia) · ❌ brak

## Narzędzia

| Komponent | Rola | Stan | Co działa | Czego brak | Weryfikacja |
|---|---|---|---|---|---|
| **Z-Sharp** (rdzeń) | lexer, parser, sema, IR, zbc, manifest | ✅ | parytet z ziarnem Rust, punkt stały s1=s2=s3 | SSA, ZAMM lowering | `scripts/tests/run_all.zsharp`, `scripts/fixpoint.zsharp` |
| stage0 (Rust) | ziarno: interpreter AST + `bundle` | ✅ | run/exec/bundle, pełny język | — (celowo ziarno) | cargo test, golden |
| runtime natywny (C) | wspólny dla C/LLVM/gcc | 🟡 | wartości, tablice (hasz+COW), napisy UTF-8, wyjątki, ~60 wbudowanych, File/Dir/Process/System | obiekty/klasy, domknięcia, GC/zwalnianie (Persistent) | testy różnicowe |
| **zsc** (kompilator) | AOT do natywnego | 🟡 | backend C ✅ i **LLVM** ✅ (`--backend llvm`, `--target`), standalone ✅ | moduły z kodem top-level → jawne `E01xx`; działają: **domknięcia, funkcje strzałkowe, named args, spread, pola statyczne, `Std.*`**, **klasy (pola, metody, `new`, konstruktor, `extends`, `parent.`, stałe, metody statyczne, interfejsy, `instanceof`), klasy wyjątków (`extends Exception/Error`, catch po klasie), `import` modułów (funkcje i klasy), wariadyczne `...$x`, operatory bitowe** | `zsc/tests/native/run_diff.sh` 30/30 |
| **zsi** (interpreter) | uruchamianie | 🟡 | launcher, `-e`, `--compile` (standalone), `--emit-zbc`, `--check` | własna VM bajtkodowa, JIT | ręcznie + `scripts/fixpoint.zsharp` |
| **zst** (transpilator) | cele php/js/c | 🟡 | js ✅ i c ✅ (podzbiór, parytet), php składniowy (niski parytet) | php semantyczny, klasy/domknięcia w js/c, py/go | `zst/tests/run_diff.sh` |
| **zzc** (frontend gcc) | gcc przez C | 🟡 | sterownik gcc: -O/-g/-flto/-S/-c, cross przez `--gcc`, runtime | prawdziwy frontend cc1/GENERIC, klasy, moduły | ręcznie (`z# build --backend gcc`) |
| **wrapper/ → z#** | jedno polecenie | ✅ | new/init/build/run/check/test/transpile/clean/doctor, 3 backendy | zależności ścieżkowe/rejestr, parser pełnego manifestu | `wrapper/tests/run.sh` 34/34 |

## Bootstrap

| Etap | Stan |
|---|---|
| zsc kompiluje zsc do `.zbc` (s1=s2=s3) | ✅ |
| binarki standalone (zsc, zsi, zst, zzc, z#) bez źródeł | ✅ (runtime = stage0) |
| kod natywny bez Rusta (C/LLVM) dla programów w podzbiorze | ✅ |
| **zsc skompilowany natywnie kompiluje zsc (stage3)** | ✅ — `scripts/stage3.zsharp`: zsc2 (z interpretera) → zsc3 → zsc4; wygenerowany C dla zsc (15 tys. linii) identyczny dla interpretera i wszystkich etapów; natywny zsc ~7× szybszy (2,3 s vs 17 s) |
| bytecode VM / JIT | ❌ |

## Kiedy poważne narzędzia w Z#?

* **Już teraz (stage0/standalone):** pełny język (klasy, moduły, domknięcia) uruchamiany przez interpreter; sprawdza się dla narzędzi CLI, skryptów, kompilatorów (zsc/zst/zzc/z# same są tego dowodem), z kosztem wydajności interpretera AST w Rust. Dystrybucja: `zsi --compile` → jeden plik.
* **Narzędzia natywne i szybkie (C/LLVM):** dziś tylko styl „funkcje + tablice + napisy + wyjątki" (jak `z#` — to realne narzędzie CLI). Do pisania typowych narzędzi z klasami i modułami potrzebne są kamienie milowe:

| # | Kamień milowy | Odblokowuje |
|---|---|---|
| M1 | ✅ `lib/link.zsharp` linkuje moduły (funkcje, klasy) dla C, LLVM, zst c/js; brak: kod top-level w module | podział na pliki, biblioteki funkcji |
| M2 | ✅ klasy (`lib/classes.zsharp`: lowering do funkcji + obiekty w runtime) i wyjątki-klasy; brak: statyczne pola, `abstract` egzekwowane, widoczność (private) nieegzekwowana | kompilatory/duże narzędzia natywnie, **stage3 ✅** |
| M3 | operatory bitowe ✅, variadic ✅, domknięcia ✅, funkcje strzałkowe ✅, named args ✅ (funkcje i metody), spread ✅ (wywołania), pola statyczne ✅, `map/filter/reduce/…` ✅; brak: spread w literale tablicy `[...$a]`, domknięcia w zst js | idiomatyczny kod, callbacki |
| M4 | zwalnianie pamięci (ZAMM lowering/RC) | długo żyjące procesy, serwery |
| M5 | stdlib ✅ (`docs/STDLIB.md`): json, toml, regex, time, path, os, net (HTTP bez TLS); brak: HTTPS, strefy czasowe, regex lookbehind/Unicode | narzędzia „na serio" |
| M6 | VM bajtkodowa + JIT w zsi | wydajny REPL / skrypty |

Orientacyjnie (przy pracy etatowej jednej osoby/zespołu z AI): M1–M3 ≈ kilka–kilkanaście sesji tej wielkości (backendy już mają fundament i testy różnicowe), M4–M5 kolejne. **Realistyczny moment „poważnych narzędzi natywnie w Z#": po M1+M2+M3 (stage3 = dowód).** Do tego czasu: poważne narzędzia pisz w pełnym Z# i dystrybuuj jako standalone.

## Aktualizacja: stage3 osiągnięty
Natywny `zsc` (C, bez interpretera w kompilatorze) kompiluje sam siebie do punktu stałego. Dla narzędzi pisanych w stylu zsc (klasy, moduły, wyjątki, tablice, napisy) droga natywna jest otwarta. Ograniczenia: brak zwalniania pamięci (M4), uproszczone `System.resolveImport` (Workspace przez `Z#.toml`), zasięg `private` nieegzekwowany, typy parametrów klas sprawdzane luźno (dowolny obiekt), błędy linkowania/lowering wskazują plik wejściowy zamiast modułu źródłowego (numer linii z modułu).
Dalej: M4 (pamięć), TLS dla `Std.Net`, VM/JIT; domknięcia w zst js.
# Status Z# i mapa drogowa (stan: sesja „niezależność")

Legenda: ✅ działa i jest testowane · 🟡 częściowo (podzbiór/ograniczenia) · ❌ brak

## Narzędzia

| Komponent | Rola | Stan | Co działa | Czego brak | Weryfikacja |
|---|---|---|---|---|---|
| **Z-Sharp** (rdzeń) | lexer, parser, sema, IR, zbc, manifest | ✅ | parytet z ziarnem Rust, punkt stały s1=s2=s3 | SSA, ZAMM lowering | `scripts/tests/run_all.zsharp`, `scripts/fixpoint.zsharp` |
| stage0 (Rust) | ziarno: interpreter AST + `bundle` | ✅ | run/exec/bundle, pełny język | — (celowo ziarno) | cargo test, golden |
| runtime natywny (C) | wspólny dla C/LLVM/gcc | 🟡 | wartości, tablice (hasz+COW), napisy UTF-8, wyjątki, ~60 wbudowanych, File/Dir/Process/System | obiekty/klasy, domknięcia, GC/zwalnianie (Persistent) | testy różnicowe |
| **zsc** (kompilator) | AOT do natywnego | 🟡 | backend C ✅ i **LLVM** ✅ (`--backend llvm`, `--target`), standalone ✅ | moduły z kodem top-level → jawne `E01xx`; działają: **domknięcia, funkcje strzałkowe, named args, spread, pola statyczne, `Std.*`**, **klasy (pola, metody, `new`, konstruktor, `extends`, `parent.`, stałe, metody statyczne, interfejsy, `instanceof`), klasy wyjątków (`extends Exception/Error`, catch po klasie), `import` modułów (funkcje i klasy), wariadyczne `...$x`, operatory bitowe** | `zsc/tests/native/run_diff.sh` 30/30 |
| **zsi** (interpreter) | uruchamianie | 🟡 | launcher, `-e`, `--compile` (standalone), `--emit-zbc`, `--check` | własna VM bajtkodowa, JIT | ręcznie + `scripts/fixpoint.zsharp` |
| **zst** (transpilator) | cele php/js/c | 🟡 | js ✅ i c ✅ (podzbiór, parytet), php składniowy (niski parytet) | php semantyczny, klasy/domknięcia w js/c, py/go | `zst/tests/run_diff.sh` |
| **zzc** (frontend gcc) | gcc przez C | 🟡 | sterownik gcc: -O/-g/-flto/-S/-c, cross przez `--gcc`, runtime | prawdziwy frontend cc1/GENERIC, klasy, moduły | ręcznie (`z# build --backend gcc`) |
| **wrapper/ → z#** | jedno polecenie | ✅ | new/init/build/run/check/test/transpile/clean/doctor, 3 backendy | zależności ścieżkowe/rejestr, parser pełnego manifestu | `wrapper/tests/run.sh` 34/34 |

## Bootstrap

| Etap | Stan |
|---|---|
| zsc kompiluje zsc do `.zbc` (s1=s2=s3) | ✅ |
| binarki standalone (zsc, zsi, zst, zzc, z#) bez źródeł | ✅ (runtime = stage0) |
| kod natywny bez Rusta (C/LLVM) dla programów w podzbiorze | ✅ |
| **zsc skompilowany natywnie kompiluje zsc (stage3)** | ✅ — `scripts/stage3.zsharp`: zsc2 (z interpretera) → zsc3 → zsc4; wygenerowany C dla zsc (15 tys. linii) identyczny dla interpretera i wszystkich etapów; natywny zsc ~7× szybszy (2,3 s vs 17 s) |
| bytecode VM / JIT | ❌ |

## Kiedy poważne narzędzia w Z#?

* **Już teraz (stage0/standalone):** pełny język (klasy, moduły, domknięcia) uruchamiany przez interpreter; sprawdza się dla narzędzi CLI, skryptów, kompilatorów (zsc/zst/zzc/z# same są tego dowodem), z kosztem wydajności interpretera AST w Rust. Dystrybucja: `zsi --compile` → jeden plik.
* **Narzędzia natywne i szybkie (C/LLVM):** dziś tylko styl „funkcje + tablice + napisy + wyjątki" (jak `z#` — to realne narzędzie CLI). Do pisania typowych narzędzi z klasami i modułami potrzebne są kamienie milowe:

| # | Kamień milowy | Odblokowuje |
|---|---|---|
| M1 | ✅ `lib/link.zsharp` linkuje moduły (funkcje, klasy) dla C, LLVM, zst c/js; brak: kod top-level w module | podział na pliki, biblioteki funkcji |
| M2 | ✅ klasy (`lib/classes.zsharp`: lowering do funkcji + obiekty w runtime) i wyjątki-klasy; brak: statyczne pola, `abstract` egzekwowane, widoczność (private) nieegzekwowana | kompilatory/duże narzędzia natywnie, **stage3 ✅** |
| M3 | operatory bitowe ✅, variadic ✅, domknięcia ✅, funkcje strzałkowe ✅, named args ✅ (funkcje i metody), spread ✅ (wywołania), pola statyczne ✅, `map/filter/reduce/…` ✅; brak: spread w literale tablicy `[...$a]`, domknięcia w zst js | idiomatyczny kod, callbacki |
| M4 | zwalnianie pamięci (ZAMM lowering/RC) | długo żyjące procesy, serwery |
| M5 | stdlib ✅ (`docs/STDLIB.md`): json, toml, regex, time, path, os, net (HTTP bez TLS); brak: HTTPS, strefy czasowe, regex lookbehind/Unicode | narzędzia „na serio" |
| M6 | VM bajtkodowa + JIT w zsi | wydajny REPL / skrypty |

Orientacyjnie (przy pracy etatowej jednej osoby/zespołu z AI): M1–M3 ≈ kilka–kilkanaście sesji tej wielkości (backendy już mają fundament i testy różnicowe), M4–M5 kolejne. **Realistyczny moment „poważnych narzędzi natywnie w Z#": po M1+M2+M3 (stage3 = dowód).** Do tego czasu: poważne narzędzia pisz w pełnym Z# i dystrybuuj jako standalone.

## Aktualizacja: stage3 osiągnięty
Natywny `zsc` (C, bez interpretera w kompilatorze) kompiluje sam siebie do punktu stałego. Dla narzędzi pisanych w stylu zsc (klasy, moduły, wyjątki, tablice, napisy) droga natywna jest otwarta. Ograniczenia: brak zwalniania pamięci (M4), uproszczone `System.resolveImport` (Workspace przez `Z#.toml`), zasięg `private` nieegzekwowany, typy parametrów klas sprawdzane luźno (dowolny obiekt), błędy linkowania/lowering wskazują plik wejściowy zamiast modułu źródłowego (numer linii z modułu).
Dalej: M4 (pamięć), TLS dla `Std.Net`, VM/JIT; domknięcia w zst js.
