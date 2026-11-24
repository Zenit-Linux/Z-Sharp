# ZAMM — zarządzanie pamięcią w runtime natywnym (M4)

> **Zgodnie ze SPEC (§1.1): bez Garbage Collectora.** Pamięć jest zarządzana przez ZAMM — łańcuchy aren (*chained arenas*) powiązane z ramkami
> wywołań i regionami pętli. Nie ma skanowania sterty, markowania ani pauz kolektora. Wcześniejsza, konserwatywna implementacja GC
> (mark & sweep) została **usunięta**; `System.gc()` istnieje już tylko jako pusta funkcja zgodności.

## Model
* Każde wywołanie funkcji (`zs_enter` … `zs_leave`) otwiera **arenę poziomu *n*** (n = głębokość ramki). Alokacja to przesunięcie wskaźnika
  (*bump*) w chunku o rozmiarze 1–16 stron; gdy chunk się skończy, arena dołącza kolejny (łańcuch). Zwolnienie ramki oddaje całe chunki naraz.
* **Poziom 0** to arena trwała (statyki, wartości globalne, wyniki przeżywające do końca procesu).
* **Tablica stron** mapuje każdą stronę na poziom, więc z samego wskaźnika da się ustalić „wiek” wartości.
* **Niezmiennik:** wartość nigdy nie wskazuje na wartość młodszą (z głębszego poziomu). Cała reszta mechanizmu istnieje, by go utrzymać.
* **Duże obiekty** (> 16 KiB) trafiają do osobnej przestrzeni (malloc) i są zwalniane razem z areną właściciela; duże napisy są przepinane
  do starszej areny zamiast kopiowane.

## Ucieczka wartości (promocja)
Gdy wartość z młodszej areny miałaby stać się osiągalna ze starszej, jest **głęboko kopiowana** do areny docelowej (kolejka robocza, bez rekurencji):

| Zdarzenie | Cel promocji |
|---|---|
| `return $x` (`zs_leave`) | arena wywołującego |
| zapis do starszego kontenera (bariera `zm_bar` w `arr_put`) | arena kontenera |
| pole statyczne (`zs_sset`) | poziom 0 |
| wyjątek (`zs_raise`) | poziom obsługującego `catch`, potem `zm_unwind` |

Obiekty mają wskaźnik przekierowania `fwd`; `obj_canon` zapewnia, że tożsamość (`===`) przeżywa promocję.

## Regiony pętli
Pętle alokujące w każdej iteracji dostają **arenę na iterację**, zwalnianą na końcu obrotu. Analiza żywotności (`lib/regions.zsharp`)
rozstrzyga, które zmienne są *przenoszone* między iteracjami (`zs_carry`, promocja do areny pętli), a które *prywatne* (zerowane na wyjściu z regionu).
Pętle wewnątrz `try` nie dostają regionów (bezpieczeństwo rozwijania stosu).

## Diagnostyka
* `ZS_MEM_STATS=1` — przy wyjściu raport: alokacje, promocje, szczyt, chunki, duże obiekty.
* `System.gcStats()` — zachowane dla zgodności (zwraca liczniki ZAMM).
* Testy: `tests/m4/zamm` (promocje, bariery, wyjątki, regiony), `tests/m4/gc_stress` (obciążeniowy; przechodzi pod ZAMM), `tests/zamm_test`.

## Zakres i ograniczenia
* Dotyczy backendów **C**, **LLVM** i **gcc** (wspólny `runtime/native/zs_rt.c`). Interpreter (stage0) liczy referencje (`Rc`) i zwalnia od razu.
* Koszt: promocja kopiuje wartości uciekające z ramki (głęboko). Dla bardzo dużych struktur zwracanych z funkcji lepiej budować je w ramce wołającej.
* Cykle referencji nie wymagają osobnej obsługi — arena zwalnia całość naraz.
* Brak współbieżności w runtime (jeden wątek), więc areny nie mają blokad.
* stage3 (natywny `zsc` kompilujący sam siebie) daje C identyczny z wynikiem referencyjnym.
