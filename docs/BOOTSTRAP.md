# Bootstrap Z# bez stage0 (Rust)

Z# buduje się sam, a droga startowa **nie wymaga Rusta**. Wystarczy jedno z czterech ziaren w `bootstrap/seed/`.

```
sh bootstrap/bootstrap.sh                 # SEED=c (domyślnie): potrzebny tylko kompilator C11 + LLVM >= 21 (llc/opt/clang)
SEED=ll  sh bootstrap/bootstrap.sh        # ziarno = LLVM IR kompilatora (llc >= 21 + kompilator C dla runtime)
SEED=js  sh bootstrap/bootstrap.sh        # ziarno = zsc przetranspilowany do JS (Node >= 16)
SEED=php sh bootstrap/bootstrap.sh        # ziarno = zsc przetranspilowany do PHP (PHP >= 8.1)
SEED=rust sh bootstrap/bootstrap.sh       # starszy interpreter stage0 (cargo) — tylko zgodność wsteczna
```

Układ: katalog z podkatalogami `Z-Sharp`, `zsc`, `zst`, `zzc`, `zsi` (nazwy z sufiksem `-main` też są rozpoznawane).
Wynik: `$OUT/bin/{zsc,zst,zzc,zsi,z#}` oraz `$OUT/share/z-sharp/` (runtime, stdlib). Narzędzia same znajdują runtime
względem swojej binarki, więc `ZS_HOME` nie jest potrzebne po instalacji.

## Etapy

| Etap | Co | Z czego |
|---|---|---|
| stage0 | `zsc` zbudowany z **ziarna** | `seed/zsc-seed.{c,ll,js,php}` |
| stage1 | `zsc` zbudowany natywnie (LLVM) | stage0 kompiluje źródła `zsc/src` |
| stage2 | j.w. | stage1 kompiluje źródła `zsc/src` |
| stage3 | j.w. | stage2 |
| stage4 | j.w. | stage3 |
| narzędzia | `zst`, `zzc`, `zsi`, `z#` | kompiluje je stage4 |
| test | zgodność (7 programów × 6–8 dróg) | tylko narzędzia natywne |

Kryteria przejścia (sprawdza skrypt):

1. LLVM IR emitowany dla `zsc` przez stage1, stage2, stage3 i stage4 jest **identyczny** (≈193 tys. linii).
2. Binarki stage2 = stage3 = stage4 (bit w bit).
3. Ziarno jest aktualne, gdy stage0 emituje ten sam IR (w przeciwnym razie ostrzeżenie; odśwież `UPDATE_SEED=1`).

Zweryfikowane: wszystkie cztery ziarna (`c`, `ll`, `js`, `php`) prowadzą do **tej samej binarki** `zsc`.
Dodatkowo `zsc` przetranspilowany przez `zst` do **JS, PHP, C, C++ i Rust** kompiluje samego siebie do identycznego IR
(samohosting każdego celu `zst`).

## Co znaczy „porzucenie stage0”

- Rust nie jest potrzebny do zbudowania ani używania żadnego narzędzia. `stage0/` i `Cargo.toml` pozostają w repozytorium
  wyłącznie jako ścieżka zgodności wstecznej (`SEED=rust`, `zsi --legacy`) oraz jako źródło oczekiwanych wyników starszych testów.
- Oczekiwane wyniki testów zgodności (`tests/conformance/*.out`, `*.rc`) są zapisane w repozytorium; testy ich nie generują.
- Aktualizacja ziaren: `UPDATE_SEED=1 sh bootstrap/bootstrap.sh` (używa stage4 i `zst`).
- Zależności zewnętrzne: kompilator C11 (runtime `zs_rt.c`) oraz **LLVM >= 21** (`opt`, `llc`, `clang` jako sterownik linkera).
  Runtime w C jest jedynym kodem nie-Z# na drodze kompilacji; `zsc` kompiluje go raz do cache i nie generuje C dla programów.

## Ograniczenia (stan zweryfikowany)

- Sprawdzone wyłącznie na **Linux x86-64** z LLVM **21.1.8**, gcc/g++ 13.3, Node 22, PHP 8.3.6, rustc 1.75.
  macOS, Windows, aarch64 i kompilacja skrośna (`--target`) nie były testowane.
- Odrzucenie za starego LLVM jest przetestowane (`zsc/tests/run.sh`); `--allow-older-llvm` obniża próg do 17 bez gwarancji.
- Samohosting przez cel Rust i C++ działa, ale budowa trwa dłużej (kilkadziesiąt sekund do kilkunastu minut zależnie od `-O`).
