# M6 — VM bajtkodowa i JIT

## VM (stage0, `stage0/zs-interp/src/vm.rs`)
Funkcje są kompilowane **leniwie** (przy pierwszym wywołaniu) do bajtkodu stosowego z lokalnymi zmiennymi w slotach; wynik trafia do cache (`vm_cache`).
Konstrukcje, których VM nie obsługuje (np. `finally`, przypisanie do statycznego `Dot`), powodują **powrót do interpretera drzewiastego** dla danej funkcji —
semantyka jest identyczna. Obsługa `try/catch` używa stosu handlerów. Sterowanie: `ZS_VM=0` wyłącza VM.

Weryfikacja: testy złote przechodzą z VM włączoną i wyłączoną (`ZS_VM=0`); `tests/m6/vm` to test różnicowy (+ natywnie w C i LLVM).
Przyspieszenie jest umiarkowane (≈ 10–25% na kodzie obliczeniowym) — główny koszt to wartości `Rc` i dynamiczne typowanie, nie dyspozycja węzłów.

## JIT w zsi: `zsi --jit plik.zsharp`
Program jest kompilowany natywnie przez `zsc build` i uruchamiany; wynik trafia do **cache** (klucz = hasz zawartości źródeł).
Gdy kompilacja się nie uda, zapisywany jest znacznik `.fail` i program działa dalej w VM (bez ponawiania prób). `zsi --clean-cache` czyści cache.

## Ograniczenia
* Pierwsze uruchomienie z `--jit` płaci koszt kompilacji; opłaca się dla programów działających dłużej.
* Natywnie nieobsługiwane: odwołania do niezdefiniowanych zmiennych/funkcji jako błędy czasu wykonania (wykrywane statycznie).
