# Status Z# (po sesji „niezależność od stage0”)

Legenda: ✅ działa i jest testowane · 🟡 częściowo · ❌ brak. Platforma weryfikacji: Linux x86-64, LLVM 21.1.8.

## Narzędzia

| Komponent | Rola | Stan | Co działa | Znane ograniczenia | Weryfikacja |
|---|---|---|---|---|---|
| **zsc** | kompilator, **wyłącznie backend LLVM >= 21** | ✅ | `build/run/check/analyze/runtime/toolchain`, emisje `exe/obj/asm/bc/ll/ir/zbc`, `-O0..-Oz`, `--target`; brak ścieżki C; klasy, domknięcia, wyjątki, moduły | wbudowane spoza podzbioru natywnego → `E0103`; tylko x86-64/Linux sprawdzone | `zsc/tests/run.sh` (20 kontroli), `tests/run_conformance.sh zsc` |
| **zst** | transpilator | ✅ | cele **js, php, c, cpp, rust** (pełny język przez linker modułów + IR), `php-src` (starszy, składniowy); `--run`, `--check`, bundle jednoplikowy | `Socket.*` tylko w c (i zsc); js: int64 przez BigInt (wolniej powyżej 2^53); rust/cpp: wolna kompilacja dużych programów | `zst/tests/run.sh`, samohosting `zsc` przez każdy cel |
| **zzc** | frontend gcc (Z# → IR → C → gcc) | ✅ | pełny język (klasy, domknięcia, importy), cache obiektu runtime kluczowany flagami, `-O/-g/-flto/-S/-c/-fsanitize`, `run`, `--dry-run` | to sterownik gcc, nie wtyczka `cc1`; jeden plik wejściowy (reszta przez `import`) | `zzc/tests/run.sh` |
| **zsi** | uruchamianie | ✅ | **bez stage0**: kompilacja natywna przez `zsc` + trwały cache (ciepłe uruchomienie ≈ 6 ms), `-e`, `--check`, `--compile`, `--emit-zbc`, `args[0]` jak w interpreterze | brak własnej VM; wykonanie `.zbc` tylko przez `--legacy` (stage0) | `zsi/tests/run.sh` |
| runtime natywny (C) | wspólny dla zsc/zzc/zst c | ✅ | patrz `docs/SPEC.md` § „Zachowanie runtime natywnego” | brak zrównoleglenia | testy zgodności |
| runtimes zst | `zs_runtime.{js,php,rs,hpp}` | ✅ | pełny zestaw wbudowanych i modułów `Math/System/File/Directory/Process` | `Socket.*` → błąd wykonania | testy zgodności |
| stage0 (Rust) | **ziarno legacy** | 🟡 | nadal buduje się i działa | niepotrzebny do bootstrapu | `SEED=rust` |
| **z#** (wrapper) | jedno polecenie | ✅ | buduje się natywnie kompilatorem stage4 | nie zmieniany w tej sesji | — |

## Bootstrap

Zobacz `docs/BOOTSTRAP.md`. Skrót: ziarno (`c|ll|js|php`) → stage1 → stage2 → stage3 → stage4, identyczny IR i binarki, narzędzia
zbudowane natywnie, test zgodności tylko narzędziami natywnymi. **Stage0 nie jest potrzebny.**

## Testy

`tests/conformance/*.zs` (7 programów: arytmetyka/int64, napisy UTF-8, tablice/COW, sterowanie, klasy/domknięcia, wyjątki, system/pliki/procesy)
z zapisanymi wynikami `.out`/`.rc`. Uruchamiane drogami: `zsc`, `zzc`, `zsi`, `zst-{js,php,c,cpp,rust}` (`tests/run_conformance.sh`).
Uwaga: odwołania do `zsc/tests/native/run_diff.sh` w starszych dokumentach dotyczą skryptu, którego nie ma w archiwum; zastępuje go powyższy zestaw.

## Różnice względem interpretera stage0 (świadome decyzje)

Zachowanie natywne jest definicją odniesienia (patrz SPEC):
`upper/lower` tylko ASCII; `replace` z pustym wzorcem zwraca tekst bez zmian; nieznana funkcja to błąd kompilacji (nie wykonania);
`Process.run` nieistniejącego polecenia daje status 127; argumenty `echo` są obliczane przed wypisaniem; odczyt niezdefiniowanej zmiennej to błąd wykonania.

## Czego nie ma / nie zweryfikowano

- Platformy inne niż Linux x86-64, kompilacja skrośna, LLVM inny niż 21.1.8.
- Python/Go jako cele `zst`.
- `Socket.*` w celach js/php/rust/cpp.
- Prawdziwy frontend `cc1` dla gcc (zzc jest sterownikiem).

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
| M3 | ✅ operatory bitowe, variadic, domknięcia, funkcje strzałkowe, named args (funkcje i metody), spread (wywołania **i literały tablic `[...$a]`** — `lib/spreadlit.zsharp`), pola statyczne, `map/filter/reduce/…`; brak: domknięcia w zst js (brak runtime JS w archiwum) | idiomatyczny kod, callbacki |
| M4 | ✅ **ZAMM**: chained arenas per ramka, promocja ucieczek, bariera zapisu, regiony pętli (`docs/MEMORY.md`, `ZS_MEM_STATS`, testy `tests/m4/`); bez GC, zgodnie ze SPEC | długo żyjące procesy, serwery |
| M5 | stdlib ✅ (`docs/STDLIB.md`): json, toml, regex, time, path, os, net (HTTP, HTTPS przez `curl`), strefy czasowe (reguły EU/US/AU/NZ), regex lookbehind, `\p{..}` i flaga `u` (przybliżone tablice Unicode) | narzędzia „na serio" |
| M6 | ✅ VM bajtkodowa w stage0 (`vm.rs`, leniwa, z fallbackiem do interpretera) + `zsi --jit` (kompilacja natywna z cache, fallback do VM) — `docs/VM.md`; przyspieszenie VM umiarkowane | wydajny REPL / skrypty |

Orientacyjnie (przy pracy etatowej jednej osoby/zespołu z AI): M1–M3 ≈ kilka–kilkanaście sesji tej wielkości (backendy już mają fundament i testy różnicowe), M4–M5 kolejne. **Realistyczny moment „poważnych narzędzi natywnie w Z#": po M1+M2+M3 (stage3 = dowód).** Do tego czasu: poważne narzędzia pisz w pełnym Z# i dystrybuuj jako standalone.

## Aktualizacja: stage3 osiągnięty
Natywny `zsc` (C, bez interpretera w kompilatorze) kompiluje sam siebie do punktu stałego. Dla narzędzi pisanych w stylu zsc (klasy, moduły, wyjątki, tablice, napisy) droga natywna jest otwarta. Ograniczenia: uproszczone `System.resolveImport` (Workspace przez `Z#.toml`), zasięg `private` nieegzekwowany, typy parametrów klas sprawdzane luźno (dowolny obiekt), błędy linkowania/lowering wskazują plik wejściowy zamiast modułu źródłowego (numer linii z modułu).
Dalej: własny TLS dla `Std.Net`, VM/JIT (M6), domknięcia w zst js, pełne tablice Unicode.
