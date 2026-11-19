# Z# — Specyfikacja języka programowania

**Wersja dokumentu:** 0.0.1 (projekt roboczy; manifest `Z#.toml`, `main` opcjonalne)
**Status:** propozycja architektury i składni
**Rozszerzenie źródeł:** `.zs`
**Narzędzia:** `zsi`, `zsc`, `zst, zzc`

> Z# to dynamicznie typowany język programowania ogólnego przeznaczenia. Jego składnia jest inspirowana PHP, ale język posiada własny frontend, reprezentację pośrednią, runtime, system pamięci i narzędzia. Ma służyć do tworzenia aplikacji konsolowych, desktopowych, serwerowych, sieciowych, gier, bibliotek oraz narzędzi systemowych.

---

## 1. Założenia i cele projektu

### 1.1. Główne założenia

1. Składnia ma być znajoma dla programistów PHP.
2. Z# jest **dynamicznie typowany**.
3. Z# ma dwa podstawowe sposoby uruchamiania:
   - `zsi` — uruchamianie kodu źródłowego przez własny bytecode VM z JIT;
   - `zsc` — kompilacja AOT przez własny backend LLVM do natywnej binarki.
   - `zst` - transpilator dla Z#.
   - `zzc` - (przyszlosciowe) projekt frontend dla gcc
4. Język nie używa Garbage Collectora.
5. Pamięć jest zarządzana automatycznie przez **Z# Adaptive Memory Management (ZAMM)**, oparte na chained arenas, analizie czasu życia i kontrolowanych mechanizmach własności.
6. Programista nie musi ręcznie tworzyć ani niszczyć aren w typowym kodzie.
7. `zsi` obsługuje shebangi.
8. Język ma własną standardową bibliotekę i nie jest nakładką na PHP.

### 1.2. Cele projektowe

- czytelna, zwięzła składnia;
- brak obowiązkowego deklarowania typów;
- deterministyczne zarządzanie zasobami;
- przewidywalne opóźnienia i brak pauz GC;
- możliwość optymalizacji kodu zarówno w JIT, jak i AOT;
- przenośność między Linuxem, Windows i macOS;
- możliwość tworzenia programów niskopoziomowych bez rezygnowania z wygody języka wysokiego poziomu.

### 1.3. Czego Z# nie powinien udawać

Brak GC nie oznacza automatycznie braku wycieków, a chained arenas nie rozwiązują samodzielnie problemu czasu życia obiektów. ZAMM musi mieć formalne reguły własności, ucieczki obiektów, referencji i niszczenia zasobów. Nie wolno zwalniać całej areny, jeżeli nadal istnieją legalne odwołania do jej danych.

---

## 2. Pliki i podstawowa postać programu

Pliki źródłowe mają rozszerzenie `.zs`.

Przykładowy `main.zs`:

```php
#!/usr/bin/env zsi

import System;

func main(): void {
    echo "Hello from Z#!";
}

main();
```

W zwykłym pliku `.zs` znaczniki `<?php` i `?>` nie są wymagane. Nie są częścią normalnej składni Z#.

`main` **nie jest wymagane**. Program może składać się z samego kodu najwyższego poziomu. Jeśli zdefiniowano `func main` i nie wywołano go jawnie, runtime wywołuje je po kodzie najwyższego poziomu; zwrócona liczba całkowita staje się kodem wyjścia procesu.

---

## 3. Komentarze

Z# używa własnych znaczników komentarzy.

| Składnia | Znaczenie |
|---|---|
| `#` | komentarz jednoliniowy |
| `##` | komentarz dokumentacyjny |
| `#( ... )#` | komentarz wieloliniowy |

### 3.1. Komentarz jednoliniowy

```php
# Komentarz
$name = "Z#"; # Komentarz na końcu instrukcji
```

### 3.2. Komentarz dokumentacyjny

```php
##
 * Oblicza sumę dwóch liczb.
 *
 * @param a Pierwsza liczba
 * @param b Druga liczba
 * @return Wynik dodawania
##
func add($a, $b) {
    return $a + $b;
}
```

Dokumentacyjny komentarz `##` powinien być możliwy do odczytania przez generator dokumentacji `zdoc`.

### 3.3. Komentarz wieloliniowy

```php
#(
    Komentarz wieloliniowy.
    Może zawierać wiele linii.
)#
```

**Propozycja dla wersji 0.3:** komentarze wieloliniowe nie są zagnieżdżane. Lexer rozpoznaje shebang wyłącznie na samym początku pliku.

---

## 4. Zmienne i dynamiczny system typowania

### 4.1. Zmienne

Zmienne używają prefiksu `$`, podobnie jak w PHP.

```php
$name = "Michał";
$age = 20;
$active = true;
$numbers = [10, 20, 30];
$nothing = null;
```

Nie jest wymagana deklaracja typu przed użyciem zmiennej. Typ wartości jest określany w czasie działania programu.

### 4.2. Wbudowane typy wartości

Proponowany zestaw typów:

| Typ | Przykład |
|---|---|
| `int` | `42` |
| `float` | `3.14` |
| `string` | `"Hello"` |
| `bool` | `true`, `false` |
| `array` | `[1, 2, 3]` |
| `object` | instancja klasy |
| `null` | `null` |
| `function` | funkcja jako wartość |
| `resource` | uchwyt do zasobu runtime |

To lista logicznych kategorii wartości. Dokładna reprezentacja wewnętrzna (tagged value, NaN-boxing lub inny wariant) wymaga decyzji implementacyjnej.

### 4.3. Dynamiczne typowanie

Ta sama zmienna może przechowywać wartości różnych typów:

```php
$value = 123;
$value = "tekst";
$value = [1, 2, 3];
```

Operacje na wartościach niezgodnych z daną operacją powinny powodować określony błąd runtime, a nie niezdefiniowane zachowanie.

### 4.4. Opcjonalne adnotacje typów

Wstępna propozycja: składnia adnotacji może być dostępna jako **runtime contract**, ale nie zmienia dynamicznego charakteru języka.

```php
func add(int $a, int $b): int {
    return $a + $b;
}
```

W takim wariancie runtime sprawdza argumenty i wartość zwracaną. Kompilator może wykorzystać te informacje do optymalizacji, ale nie wymaga od programisty statycznego typowania całego programu.

Trzeba jeszcze zdecydować, czy adnotacje typów będą:
- całkowicie opcjonalne i sprawdzane w runtime;
- jedynie wskazówkami dla optymalizatora;
- czy też Z# w ogóle zrezygnuje z adnotacji typów w składni.

---

## 5. Literały, tablice i operatory

### 5.1. Tablice indeksowane

```php
$numbers = [10, 20, 30];

echo $numbers[0];
```

### 5.2. Tablice asocjacyjne

```php
$user = [
    "name" => "Michał",
    "age" => 20
];

echo $user["name"];
```

### 5.3. Operatory

Z# zachowuje wiele operatorów znanych z PHP.

| Operator | Znaczenie |
|---|---|
| `+`, `-`, `*`, `/`, `%`, `**` | działania arytmetyczne |
| `.` | konkatenacja tekstu |
| `==`, `!=` | porównanie wartości według reguł Z# |
| `===`, `!==` | ścisłe porównanie wartości i typu |
| `<`, `>`, `<=`, `>=` | porównania |
| `&&`, `||`, `!` | logika |
| `and`, `or`, `xor` | alternatywne operatory logiczne, jeśli zostaną zachowane |
| `??`, `??=` | null coalescing |
| `=>` | para klucz-wartość |
| `->` | dostęp do składowej obiektu |
| `.` | dostęp do modułu i API |
| `++`, `--` | inkrementacja i dekrementacja |

Dokładny priorytet i łączność operatorów muszą zostać zapisane w formalnej gramatyce. Nie należy zakładać, że wszystkie priorytety PHP zostaną automatycznie skopiowane bez osobnej decyzji.

---

## 6. Funkcje

Słowem kluczowym do deklarowania funkcji jest `func`, a nie `function`.

```php
func greet($name) {
    return "Hello, " . $name;
}

echo greet("Michał");
```

### 6.1. Typ zwracany — opcjonalna adnotacja

```php
func add(int $a, int $b): int {
    return $a + $b;
}
```

Jeżeli adnotacje zostaną zachowane, będą kontraktami sprawdzanymi w runtime, chyba że późniejsza decyzja projektowa ustali inaczej.

### 6.2. Wartości domyślne

```php
func greet($name = "World") {
    echo "Hello, " . $name;
}
```

### 6.3. Funkcje anonimowe

```php
$multiply = func($a, $b) {
    return $a * $b;
};
```

### 6.4. Funkcje strzałkowe

```php
$square = fn($x) => $x * $x;
```

### 6.5. Parametry nazwane i zmienna liczba argumentów

Możliwe do zachowania rozwiązania znane z PHP:

```php
func createUser($name, $age = 18) {
    return User.create($name, $age);
}

$user = createUser(name: "Michał", age: 20);
```

Składnia parametrów nazwanych i variadic wymaga ujęcia w gramatyce.

---

## 7. Instrukcje warunkowe i pętle

### 7.1. Warunki

```php
if ($age >= 18) {
    echo "Dorosły";
} elseif ($age >= 13) {
    echo "Nastolatek";
} else {
    echo "Dziecko";
}
```

### 7.2. `switch`

```php
switch ($command) {
    case "start":
        echo "Start";
        break;

    case "stop":
        echo "Stop";
        break;

    default:
        echo "Unknown command";
}
```

### 7.3. `match`

```php
$result = match ($command) {
    "start" => "Starting",
    "stop" => "Stopping",
    default => "Unknown"
};
```

### 7.4. Pętle

```php
for ($i = 0; $i < 10; $i++) {
    echo $i;
}

while ($condition) {
    # ...
}

do {
    # ...
} while ($condition);

foreach ($items as $item) {
    echo $item;
}
```

---

## 8. Dostęp do metod, właściwości i modułów

Z# używa kropki do dostępu do funkcji modułów i składowych statycznych.

Zamiast:

```php
System::cpuCount();
```

Zapis:

```php
System.cpuCount();
```

Przykłady:

```php
echo System.os();
echo System.architecture();
echo System.cpuCount();

echo Math.sqrt(25);

$version = System.version;
```

Dla obiektów proponowana jest składnia z `->`, zachowana z PHP:

```php
$user = User.create("Michał");

echo $user->name;
$user->rename("Jan");
```

W ten sposób:
- `System.cpuCount()` — funkcja lub metoda modułu;
- `Network.Http.get()` — funkcja w zagnieżdżonym module;
- `$user->name` — właściwość instancji;
- `$user->rename()` — metoda instancji.

**Nierozstrzygnięte:** czy obiekty również mają używać kropki (`$user.name`), czy pozostaje PHP-owe `->`. W przykładach tej specyfikacji pozostaje `->`, żeby odróżnić dostęp do modułów od dostępu do instancji.

---

## 9. Moduły i importowanie

### 9.1. Import modułu

```php
import Math;
import File;
import Network.Http;
import System.Process;
```

Kropka oddziela kolejne poziomy ścieżki modułu.

### 9.2. Import pliku

```php
import Utils.file;
import Database.connection;
import Graphics.renderer;
```

Proponowana konwencja mapowania:
- `import Math;` — wyszukanie modułu `Math` zgodnie z regułami resolvera;
- `import Network.Http;` — moduł `Http` wewnątrz `Network`;
- `import Utils.file;` — plik/moduł `file.zs` w katalogu `Utils`.

### 9.3. Przykładowa struktura projektu

```text
MyProject/
├── Z#.toml
├── main.zs
├── Math.zs
├── Network/
│   ├── Http.zs
│   └── Socket.zs
└── Utils/
    ├── file.zs
    └── string.zs
```

### 9.4. Przestrzenie nazw

Wstępnie:

```php
namespace Network.Http;

func get($url) {
    # ...
}
```

Resolver importów, zasady widoczności (`public`/`private`) i zachowanie przy konfliktach nazw wymagają osobnego projektu.

---

## 10. Programowanie obiektowe

Z# ma wspierać:
- klasy i obiekty;
- konstruktory i destruktory;
- dziedziczenie;
- interfejsy;
- metody i właściwości;
- widoczność składowych;
- metody statyczne;
- klasy abstrakcyjne;
- trait-y lub równoważny mechanizm ponownego użycia kodu.

Przykład:

```php
interface Animal {
    public function speak();
}

class Dog implements Animal {
    private $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function speak() {
        return "Woof! I am " . $this->name;
    }
}

$dog = new Dog("Rex");
echo $dog->speak();
```

**Uwaga:** przykład zachowuje `public function` jako składnię PHP dla metod interfejsu i klasy. Należy zdecydować, czy Z# zmienia również deklaracje metod na `public func`, `private func` itd. Dla spójności języka rekomendowane jest `public func`.

---

## 11. Obsługa błędów i zasobów

### 11.1. Wyjątki

```php
try {
    $result = calculate();
} catch (Exception $e) {
    echo $e->getMessage();
} finally {
    echo "Finished";
}
```

### 11.2. Deterministyczne zarządzanie zasobami

Zasoby takie jak pliki, sockety i blokady muszą być zwalniane deterministycznie.

Możliwa składnia:

```php
using File.open("data.txt", "r") as $file {
    $contents = $file.read();
}
```

`using` powinno gwarantować wykonanie procedury zwalniającej również przy wyjątku lub wcześniejszym wyjściu z bloku.

Nie należy utożsamiać zwalniania zasobów z odzyskiwaniem pamięci obiektów. To dwa powiązane, ale odrębne zagadnienia.

---

## 12. ZAMM — Z# Adaptive Memory Management

### 12.1. Cel

ZAMM jest automatycznym systemem zarządzania pamięcią Z#. Użytkownik nie tworzy zwykle aren ręcznie. Runtime i kompilator dobierają strategię alokacji na podstawie rozmiaru, kontekstu, czasu życia i sposobu używania danych.

ZAMM **nie jest Garbage Collectorem**. Nie wykonuje okresowego skanowania całej sterty w celu odnalezienia nieosiągalnych obiektów.

### 12.2. Główne elementy

1. **Static Memory Analyzer** — analizuje przepływ wartości, czas życia i ucieczkę obiektów, gdy to możliwe.
2. **Adaptive Allocator** — wybiera strategię alokacji.
3. **Chained Arena Manager** — zarządza arenami i łańcuchami chunków.
4. **Thread-local Memory Pools** — ograniczają współdzielenie alokatora pomiędzy wątkami.
5. **Shared Memory Manager** — obsługuje dane współdzielone i transfer własności.
6. **Large Object Space** — obsługuje duże bufory i alokacje.
7. **Deterministic Resource Manager** — obsługuje destruktory i zasoby zewnętrzne.

### 12.3. Proponowane klasy alokacji

| Klasa | Typowe zastosowanie |
|---|---|
| Stack / frame allocation | dane o czasie życia ograniczonym do ramki wywołania |
| Thread-local arena | dane lokalne dla wątku |
| Temporary arena | krótkotrwałe dane tymczasowe |
| Task arena | dane związane z zadaniem |
| Persistent arena | dane o dłuższym czasie życia |
| Shared allocation | dane współdzielone |
| Large Object Space | duże bufory i bloki |

To są klasy strategii, nie obietnica, że każda z nich musi być osobnym, publicznym typem w języku.

### 12.4. Chained arenas

Arena rezerwuje chunki pamięci. Alokacja może odbywać się przez szybkie przesunięcie wskaźnika w bieżącym chunku. Gdy chunk się zapełni, manager dołącza następny.

Po zakończeniu czasu życia całej grupy danych można zwolnić lub zresetować cały łańcuch, zamiast zwalniać każdy mały obiekt osobno.

### 12.5. Automatyczne dobieranie strategii

ZAMM może brać pod uwagę:
- rozmiar obiektu;
- miejsce alokacji;
- czas życia wynikający z analizy przepływu;
- czy wartość opuszcza funkcję lub zadanie (escape analysis);
- czy dane są współdzielone między wątkami;
- wymagania wyrównania pamięci;
- przewidywaną liczbę i częstotliwość alokacji.

Nie należy opierać decyzji o czasie życia wyłącznie na rozmiarze obiektu. Mały obiekt może żyć przez cały czas działania programu, a duży bufor może być tymczasowy.

### 12.6. Analiza własności i czasu życia

Podstawowy mechanizm powinien łączyć:
- escape analysis;
- analizę przepływu danych;
- analizę własności;
- wnioskowanie o czasie życia aren;
- jawne reguły transferu własności;
- bezpieczne uchwyty dla przypadków dynamicznych.

Przykład:

```php
func calculate() {
    $numbers = [10, 20, 30];
    $result = 0;

    foreach ($numbers as $number) {
        $result += $number;
    }

    return $result;
}
```

Jeżeli kompilator potwierdzi, że tablica nie ucieka poza funkcję, jej pamięć może zostać odzyskana wraz z zakończeniem odpowiedniego zakresu.

W przypadku obiektu zwracanego z funkcji jego czas życia musi zostać przedłużony lub własność przeniesiona do wywołującego.

### 12.7. Dynamiczne typowanie a pamięć

Dynamiczne typowanie utrudnia statyczne ustalanie dokładnego układu i czasu życia wszystkich wartości. Dlatego ZAMM powinien korzystać z połączenia:
- informacji statycznych dostępnych kompilatorowi;
- metadanych runtime;
- bezpiecznych uchwytów do obiektów, których czas życia nie może być dowiedziony statycznie;
- kontrolowanych mechanizmów współdzielenia.

Wstępna rekomendacja to model hybrydowy: areny i analiza statyczna jako ścieżka podstawowa, a kontrolowane referencje/uchwyty jako ścieżka dla dynamicznych przypadków.

### 12.8. Cykliczne referencje

Jeśli runtime użyje zliczania referencji jako mechanizmu pomocniczego, musi rozwiązać problem cykli. Możliwe rozwiązania:
- unikanie cykli przez reguły własności;
- słabe referencje;
- jawne obszary życia grafów obiektów;
- specjalny mechanizm wykrywania cykli, który nie może stać się ukrytym, nieprzewidywalnym GC.

To jest jedna z najważniejszych decyzji do podjęcia przed implementacją.

### 12.9. Cele wydajnościowe

ZAMM powinien dążyć do:
- braku pauz GC;
- stałokosztowej lub zbliżonej do stałokosztowej alokacji w typowych ścieżkach;
- ograniczenia blokad między wątkami;
- ponownego wykorzystania chunków;
- kontroli fragmentacji;
- deterministycznego niszczenia zasobów.

Nie należy deklarować „braku pauz” w ogóle: system operacyjny, stronicowanie, blokady, alokacje dużych bloków i inne operacje nadal mogą powodować opóźnienia. Celem jest brak globalnych pauz wynikających z GC oraz przewidywalne zarządzanie pamięcią.

---

## 13. Współbieżność i asynchroniczność

Z# ma docelowo wspierać:
- wątki;
- zadania;
- asynchroniczność;
- synchronizację;
- bezpieczne przekazywanie danych między wątkami.

Przykładowa, proponowana składnia:

```php
$task1 = Task.run(func() {
    return 100 + 200;
});

$task2 = Task.run(func() {
    return 300 + 400;
});

$result1 = await $task1;
$result2 = await $task2;

echo $result1 + $result2;
```

`Task`, `await` i model współbieżności są propozycją API i konstrukcji językowych, a nie zatwierdzoną implementacją.

ZAMM musi określić, czy zadanie:
- otrzymuje własną arenę;
- dziedziczy kontekst pamięci;
- może przejąć własność danych;
- może współdzielić dane wyłącznie przez bezpieczne uchwyty.

---

## 14. Standardowa biblioteka

Z# powinien dostarczać standardowe API niezależne od PHP.

### 14.1. System

```php
echo System.os();
echo System.architecture();
echo System.cpuCount();
echo System.totalMemory();
```

### 14.2. Pliki

```php
$file = File.open("data.txt", "w");
$file.write("Hello from Z#");
$file.close();

File.exists("data.txt");
Directory.create("my_folder");
```

### 14.3. Procesy

```php
$process = Process.start("ls", ["-la"]);
echo $process.output();
$process.wait();
```

### 14.4. Sieć

```php
$server = HttpServer.create("127.0.0.1", 8080);

$server.get("/", func($request, $response) {
    $response.text("Hello from Z#!");
});

$server.listen();
```

### 14.5. Matematyka

```php
$value = Math.sqrt(25);
```

### 14.6. GUI

```php
$app = Desktop.create("My Z# Application");

$window = $app.window([
    "title" => "Hello Z#",
    "width" => 800,
    "height" => 600
]);

$window.show();
$app.run();
```

Powyższe przykłady definiują kierunek projektowania API. Nie oznaczają, że te moduły już istnieją.

---

## 15. Shebangi

`zsi` rozpoznaje shebang na pierwszej linii pliku:

```php
#!/usr/bin/env zsi

echo "Hello from Z#!";
```

Uruchomienie na systemie Unix:

```bash
chmod +x script.zs
./script.zs
```

Interpreter powinien poprawnie obsłużyć ścieżkę do własnego pliku wykonywalnego i argumenty przekazane skryptowi.

---

## 16. Model wykonania: zsi

`zsi` uruchamia program bez wcześniejszego tworzenia samodzielnej binarki.

Przykład:

```bash
zsi main.zs
```

Proponowany pipeline:

```text
Source (.zs)
    |
    v
Lexer
    |
    v
Parser
    |
    v
AST
    |
    v
Semantic Analysis
    |
    v
Z# IR / Bytecode
    |
    v
Z# Virtual Machine
    |
    v
JIT Compiler
    |
    v
Native Machine Code
```

`zsi` powinien:
- kompilować źródło do własnego bytecode;
- wykonywać bytecode w VM;
- profilować często wykonywane fragmenty;
- kompilować wybrane fragmenty przez JIT;
- zarządzać runtime i ZAMM;
- obsługiwać shebangi.

Wymaga decyzji, czy JIT będzie kompilował metodę/funkcję, podstawowy blok, czy większe regiony kodu.

---

## 17. Model kompilacji: zsc

`zsc` kompiluje program do natywnej binarki.

Przykład:

```bash
zsc main.zs
zsc main.zs -o application
zsc main.zs --release
zsc main.zs --debug
zsc main.zs --target x86_64-linux
zsc main.zs --target aarch64-linux
```

Proponowany pipeline:

```text
Z# Source
    |
    v
Lexer
    |
    v
Parser
    |
    v
AST
    |
    v
Semantic Analysis
    |
    v
Z# IR
    |
    v
Z# LLVM Backend
    |
    v
LLVM IR
    |
    v
LLVM Optimization Pipeline
    |
    v
Object Code
    |
    v
Linker
    |
    v
Native Executable
```

### 17.1. Własny backend LLVM

Z# ma mieć własny, niezależny backend generujący LLVM IR. Oznacza to:
- własne lowering z Z# IR do LLVM IR;
- własne mapowanie wartości i operacji;
- własną integrację z ABI;
- własne decyzje dotyczące optymalizacji;
- własną obsługę runtime i linkowania.

LLVM jest infrastrukturą do optymalizacji i generowania kodu maszynowego. Nie oznacza to konieczności pisania od zera całego generatora instrukcji dla każdej architektury.

### 17.2. Poziomy optymalizacji

| Opcja | Znaczenie |
|---|---|
| `-O0` | brak lub minimalne optymalizacje |
| `-O1` | podstawowe optymalizacje |
| `-O2` | standardowe optymalizacje |
| `-O3` | agresywne optymalizacje |
| `-Os` | optymalizacja rozmiaru |

### 17.3. Runtime w binarce

Należy zdecydować, czy programy AOT:
- linkują runtime statycznie;
- wymagają współdzielonej biblioteki runtime;
- czy oferują oba tryby.

---

## 18. Z# IR — wspólna reprezentacja pośrednia

Z# powinien posiadać własne IR wspólne dla `zsi` i `zsc`.

Frontend (lexer, parser i analiza semantyczna) jest wspólny. Następnie:
- backend bytecode generuje kod dla VM/JIT;
- backend LLVM generuje LLVM IR dla kompilacji AOT.

Z# IR powinno reprezentować:
- wartości dynamiczne;
- operacje na wartościach;
- wywołania funkcji;
- przepływ sterowania;
- dostęp do właściwości i tablic;
- alokacje i operacje ZAMM;
- wyjątki;
- zadania i operacje asynchroniczne;
- informacje o typach znane z analizy lub adnotacji.

Należy rozważyć SSA jako wewnętrzną formę IR albo etap optymalizacyjny. Nie musi to być składnia widoczna dla programisty.

---

## 19. Narzędzia ekosystemu

| Narzędzie | Przeznaczenie |
|---|---|
| `zsi` | interpreter VM + JIT |
| `zsc` | kompilator AOT do binarki |
| `zpm` | menedżer pakietów |
| `zfmt` | formatter |
| `zlint` | statyczna analiza i lint |
| `ztest` | uruchamianie testów |
| `zdoc` | generator dokumentacji |

### 19.1. Przykładowe komendy

```bash
zsi main.zs
zsc main.zs -o app
zsc main.zs --release
ztest
zfmt src/
zlint src/
zpm install package-name
```

### 19.2. Struktura projektu

```text
MyProject/
├── Z#.toml
├── src/
│   ├── main.zs
│   ├── Math.zs
│   └── Utils.zs
├── tests/
│   └── MainTest.zs
├── lib/
└── build/
```

Przykładowy manifest (`Z#.toml`, TOML 1.0 — podzbiór bez napisów wieloliniowych i dat):

```toml
[package]
name    = "MyProject"
version = "1.0.0"          # MAJOR.MINOR.PATCH
kind    = "bin"            # bin | lib   (bin wymaga `entry`)
entry   = "src/main.zs"

[workspace.members]        # import Workspace.<nazwa>;  (ścieżki względem manifestu)
parser = "lib/parser.zs"
lexer  = "lib/lexer.zs"

[dependencies]
Z-Sharp = { git = "https://github.com/HackerOS-Linux-System/Z-Sharp", path = "Z-Sharp" }

[extern.sqlite]            # statyczne linkowanie bibliotek Rust/C/C++ (system `extern`)
kind = "static"            # static | dylib
path = "vendor/libsqlite3.a"

[memory]                   # konfiguracja ZAMM
chunk-kib = 64
large-object-kib = 256
```

Rozwiązywanie `import Workspace.x`: `ZS_WORKSPACE`, najbliższy `Z#.toml`, zależności `path`, katalogi nadrzędne
(oraz `Z-Sharp/` w nich) — pierwszy manifest z `[workspace.members]` wygrywa. Rozszerzenia źródeł próbowane
w kolejności: `.zsharp`, `.zs`.

Lockfile (`Z#.lock`), wersjonowanie zależności i źródła pakietów wymagają osobnej specyfikacji.

---

## 20. Przykładowy kompletny program

```php
#!/usr/bin/env zsi

import System;
import Math;
import File;

##
 * Demonstracyjny program Z#.
##
func main(): void {
    $name = "Z#";
    $numbers = [10, 20, 30];

    $sum = 0;

    foreach ($numbers as $number) {
        $sum += $number;
    }

    echo "Hello from " . $name;
    echo "CPU cores: " . System.cpuCount();
    echo "Square root: " . Math.sqrt(25);
    echo "Sum: " . $sum;
}

main();
```

---

## 21. Podsumowanie przyjętych decyzji

| Obszar | Aktualne założenie |
|---|---|
| Nazwa | Z# |
| Rozszerzenie | `.zs` |
| Inspiracja składnią | PHP |
| Typowanie | **Dynamiczne** |
| Zmienne | `$variable` |
| Funkcje | `func` |
| Dostęp do modułów | `Module.function()` |
| Dostęp do instancji | `$object->method()` — do potwierdzenia |
| Import modułów | `import Network.Http;` |
| Import plików | `import Utils.file;` |
| Komentarz liniowy | `#` |
| Komentarz dokumentacyjny | `##` |
| Komentarz wieloliniowy | `#( ... )#` |
| Interpreter | `zsi` |
| Wykonywanie w interpreterze | własny bytecode VM + JIT |
| Kompilator | `zsc` |
| Kompilacja AOT | własny backend LLVM |
| GC | brak |
| Pamięć | ZAMM + chained arenas |
| Ręczne areny | nie są wymagane w typowym kodzie |
| Shebang | obsługiwany przez `zsi` |
| Paradygmaty | proceduralny, obiektowy, funkcyjny |
| Zastosowanie | język ogólnego przeznaczenia |

---

# 22. Otwarte decyzje projektowe

Poniższe kwestie trzeba rozstrzygnąć przed zamrożeniem specyfikacji i rozpoczęciem implementacji. Niektóre z nich wpływają na parser, ABI, runtime i bezpieczeństwo pamięci.

## 22.1. Składnia dostępu do obiektów

Czy obiekty mają używać PHP-owego `->`?

```php
$user->name;
$user->rename("Jan");
```

Czy wolisz kropkę również dla obiektów?

```php
$user.name;
$user.rename("Jan");
```

**Rekomendacja:** kropka dla wszystkiego. Parser rozróżnia moduł od instancji na podstawie kontekstu i symboli. To daje najbardziej jednolitą składnię.

## 22.2. Adnotacje typów w dynamicznym języku

Czy dopuszczamy:

```php
func add(int $a, int $b): int {
    return $a + $b;
}
```

**Rekomendacja:** tak, ale są opcjonalnymi kontraktami runtime. Język nadal jest dynamicznie typowany.

## 22.3. Cykliczne referencje

Co ma się stać, gdy dwa obiekty wskazują na siebie nawzajem i nie są już potrzebne?

Możliwości:
- zakaz cykli w określonych obszarach własności;
- słabe referencje;
- jawne zarządzanie cyklicznymi grafami;
- pomocniczy mechanizm wykrywania cykli.

**Rekomendacja:** silne i słabe referencje oraz reguły własności, które ograniczają przypadkowe cykle. Nie wprowadzałbym ukrytego cyklicznego GC.

## 22.4. Model współdzielenia obiektów między wątkami

Czy obiekty można swobodnie współdzielić między wątkami, czy transfer musi być jawny?

**Rekomendacja:** domyślnie dane lokalne dla wątku; współdzielenie przez jawne bezpieczne uchwyty lub przeniesienie własności.

## 22.5. Obsługa wartości `null`

Czy Z# ma zachowywać luźne zachowanie PHP, czy wprowadzić bardziej rygorystyczne reguły?

**Rekomendacja:** `null` jest zwykłą wartością dynamiczną, ale dostęp do właściwości `null` powoduje kontrolowany błąd runtime. Operator `??` służy do obsługi braku wartości.

## 22.6. Kompilacja natywna i runtime

Czy binarka z `zsc` ma być samodzielna, z runtime dołączonym do pliku, czy może zależeć od biblioteki współdzielonej?

**Rekomendacja:** oba tryby — domyślnie wygodna binarka samodzielna, opcjonalnie współdzielony runtime dla mniejszych plików i wielu aplikacji.

## 22.7. Zgodność z PHP

Czy Z# ma zachować możliwie dużo konstrukcji PHP, czy tylko ogólną stylistykę?

**Rekomendacja:** zachować rozpoznawalne konstrukcje, ale nie dziedziczyć automatycznie niejednoznaczności i historycznych ograniczeń PHP.

---

## 23. Zalecana kolejność implementacji

1. Formalna gramatyka języka i lexer.
2. Parser i AST.
3. Semantyka dynamicznych wartości i operacji.
4. Z# IR.
5. Bytecode i podstawowa VM dla `zsi`.
6. Runtime i pierwsza wersja ZAMM.
7. Funkcje, tablice, klasy i moduły.
8. JIT.
9. Backend LLVM i `zsc`.
10. Standardowa biblioteka.
11. Wątki i asynchroniczność.
12. Menedżer pakietów i narzędzia developerskie.

**Najważniejsza rada architektoniczna:** najpierw zaprojektować semantykę wartości, własności i czasu życia. Dopiero potem optymalizować ZAMM. Dynamiczny język bez GC jest wykonalny, ale nie może polegać na zgadywaniu, kiedy obiekt przestaje być potrzebny.

dodatkowo Z# posiada system extern wpisujesz extern i tam linkujesz czy cos tam z biblioteki a on statycznie linkuje bez dyskusji ten system extern jest skrojony idealnie pod liby rusta/c/c++
Z# glownie bedzie uzywany w dystrybucji linuxa Zenit Linux.

dodatkowo Z# ma w importach obsluge rowniez workspacow (troche inspiracja rustem nie powiem) widze to tak przyklady:
import Workspace.nazwaczlonka;
import Workspace.parser;
import Workspace.lexer;

stage0 plan jest taki ze bedzie napisany w rust (bedzie to sam zsc) natomiast zst, zsi, zzc od poczatku beda pisane w Z# dodatkowo strukture widze rozdzielona na kilka repozytoriow:
1. Z-Sharp - glowny kod zrodlowy bezuzyteczny jak kompilujesz samo to sa takie podwaliny jest tam stage0 i inne pliku
2. zsc - osobne repozytorium githuba skoncentrowane na niezaleznym backendzie llvm do budowy ze zrodel klonuje Z-Sharp i uzywa go import Workspace.nazwa czlonka
3. zst - tak samo jak zsc roznica jest tylko taka ze to transpilator
4. zsi - tak jak zsc tez osobne repo ale to jest interpreter jit (wydajnosciowe ambicje gdzies w okolicach lua jit)
5. zzc - tak samo jak zsc ale cos czuje ze tu bedzie trzeba kombinowac skoro to frontend dla gcc ale jakos damy rade w prszyszlosci


---

## Dodatek: wyjście (`echo`, `necho`, `eecho`) — stage0 0.0.1

- `echo a, b, c;` — wypisuje argumenty rozdzielone pojedynczą spacją i kończy nową linią.
- `necho(a, b)` — wypisuje teksty jeden za drugim, bez separatora i bez nowej linii.
- `eecho(a, b)` — jak `echo`, ale na standardowe wyjście błędów.
- Tekst wartości: `null`, `true`/`false`, liczby całkowite, float z `.0` dla całkowitych < 1e15, tablice jako `[1, "a"]`.
