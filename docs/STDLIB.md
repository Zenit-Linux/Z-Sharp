# Biblioteka standardowa Z# (`Std.*`) — M5

Minimalistyczna: tylko to, czego potrzebują narzędzia i skrypty. Siedem modułów, zero zależności zewnętrznych.
Import: `import Std.Json;` — nazwa modułu w kodzie to ostatni segment (`Json.decode(...)`).

| Moduł | Plik | Zakres |
|---|---|---|
| `Std.Json` | `stdlib/json.zsharp` | JSON (RFC 8259): `decode`, `encode`, `encodePretty`, `valid` |
| `Std.Toml` | `stdlib/toml.zsharp` | TOML 1.0: `decode`, `encode` |
| `Std.Regex` | `stdlib/regex.zsharp` | wyrażenia regularne: `compile`, `test`, `fullMatch`, `find`, `findAll`, `replace`, `split`, `escape` |
| `Std.Time` | `stdlib/time.zsharp` | zegar, kalendarz, ISO 8601, `strftime`/`strptime`, czasy trwania |
| `Std.Path` | `stdlib/path.zsharp` | ścieżki POSIX (czysty tekst) |
| `Std.Os` | `stdlib/os.zsharp` | system: środowisko, pliki, procesy, `which` |
| `Std.Net` | `stdlib/net.zsharp` | TCP/UDP/DNS, URL, klient i serwer HTTP/1.1 (bez TLS) |

Gdzie leży kod: stage0 (Rust) wkompilowuje `stdlib/*.zsharp` w binarkę i przy imporcie rozpakowuje je do `~/.cache/z-sharp/`;
backendy natywne (C, LLVM) czytają je z `$ZS_STD` albo `$ZS_HOME/stdlib`. Wszystkie moduły działają identycznie w interpreterze
i w kodzie natywnym — sprawdza to `scripts/tests/run_all.zsharp --native`.

## Prymitywy systemowe (wbudowane moduły)
Std.* stoi na małym zestawie prymitywów obecnych w interpreterze **i** w runtime C (`runtime/native/zs_rt.c`):
`System.{setEnv,unsetEnv,environ,cwd,chdir,pid,hostname,nowNanos,monotonic,sleep,localOffset,bytes,fromBytes}`,
`File.{stat,rename,realpath,chmod,symlink,readlink}`, `Directory.remove`, oraz moduł `Socket`
(`tcpConnect, tcpListen, accept, read, write, close, setTimeout, udpBind, sendTo, recvFrom, resolve, localAddr, peerAddr`).
Moduł `Socket` jest surowy; zwykły kod używa `Std.Net`.

## Std.Json
`decode($tekst)` → obiekt = tablica asocjacyjna, tablica = lista, liczba całkowita = `int`, reszta = `float`. `encode($v)`, `encodePretty($v, $wcięcie = 2)`.
Tablica Z# jest zapisywana jako lista JSON, gdy jej klucze to `0..n-1`; pusta tablica to `[]`. Błędy: `Exception` z liniami i kolumną.
Poprawne pary zastępcze `😀` są łączone; głębokość ograniczona do 512.

## Std.Toml
`decode($tekst)` → tablica asocjacyjna (tabele, tablice tabel `[[a.b]]`, tabele inline, klucze kropkowane, teksty wielolinijkowe i surowe,
liczby `0x/0o/0b/_`, `inf`/`nan`). `encode($tablica)` → TOML (skalary → tabele → tablice tabel).
Daty i czasy TOML są zwracane jako **teksty** w oryginalnym zapisie (parsuje je `Time.parseIso`). Pusta tabela i pusta tablica są nierozróżnialne.

## Std.Regex
Silnik z cofaniem (VM, bez rekursji w dopasowaniu). Składnia: `. [..] [^..] ^ $ \b \B | (..) (?:..) (?<nazwa>..) (?=..) (?!..)`,
kwantyfikatory `* + ? {n} {n,} {n,m}` (+ leniwe `?`), klasy `\d \w \s \D \W \S` (ASCII), `\n \t \r \f \v \0 \xHH \uHHHH`, odwołania `\1..\9`.
Flagi (`compile($wzorzec, "ims")`): `i` (wielkość liter ASCII), `m` (`^`/`$` na liniach), `s` (`.` obejmuje `\n`).
Wynik `find`: `["text","start","end","groups","named"]` — **indeksy znaków**, nie bajtów. `replace` rozumie `$1 ${1} ${nazwa} $& $$`.
Brak lookbehind i klas Unicode (`\p{..}`); katastrofalne wzorce kończą się wyjątkiem po 5 mln kroków zamiast zawieszenia.
Uwaga: `match` jest słowem kluczowym Z#, dlatego funkcja nazywa się `find`.

## Std.Time
Moment = sekundy uniksowe (UTC); „części” = `["year","month","day","hour","minute","second","nano","offset"]`.
Zegar: `nowSeconds, nowMillis, nowNanos, monotonic, sleep($ms), localOffset`. Kalendarz: `make, fromUnix, toUnix, local, weekday, yearDay, isLeap, daysInMonth`.
Arytmetyka: `add, addDays, addMonths (przycina dzień), diff, compare, startOfDay`. Tekst: `toIso, parseIso, format, parse` (`%Y %m %d %H %M %S %f %N %p %a %A %b %B %j %z %:z %Z %s %F %T …`),
`formatDuration`, `parseDuration` (milisekundy). Brak nazwanych stref czasowych (tylko offsety); nazwy dni/miesięcy po angielsku.

## Std.Path
`isAbs, parts, normalize, join(...), dirname, basename, ext, stem, withExt, split, relative, absolute, isSubpath, expandHome`.

## Std.Os
`platform, arch, cpus, hostname, pid, args, env, setEnv, unsetEnv, environ, cwd, chdir, home, tempDir, exit, stat, isFile, isDir, exists, size,
readFile, readLines, writeFile, appendFile, mkdir, remove, removeAll, rename, copy, list, walk, realpath, chmod, symlink, readlink, mkdtemp,
run, exec, shell, which`. `run` zbiera `["status","stdout","stderr"]`, `exec` dziedziczy strumienie.

## Std.Net
TCP: `connect, listen, accept, read, readAll, write, close, setTimeout, localAddr, peerAddr`; UDP: `udpBind, sendTo, recvFrom`; DNS: `resolve`.
URL: `parseUrl, urlEncode, urlDecode, buildQuery, parseQuery`.
HTTP: `request($metoda, $url, $opcje)`, `get`, `post`, `put`, `delete` → `["status","reason","headers","body","url"]`
(przekierowania, `Content-Length`, `chunked`, `json => wartość`, nagłówki z kluczami małymi literami).
Serwer: `serve($host, $port, $handler, $maxŻądań = 0)`, `readRequest`, `response($status, $nagłówki, $treść)`; `$handler($req)` zwraca tekst albo `response(...)`.

### Ograniczenia sieci (świadome)
* **Brak HTTPS/TLS** — `https://` kończy się czytelnym wyjątkiem. TLS wymagałby zależności; do HTTPS użyj `curl` przez `Os.run`.
* Dane są tekstem UTF-8; treści binarne nie są bezpieczne (nieprawidłowe sekwencje zamieniane na U+FFFD). `Socket.read` dobiera brakujące bajty
  niepełnego znaku na końcu porcji, więc wielobajtowe znaki nie są uszkadzane na granicy odczytu.
* Serwer jest jednowątkowy i obsługuje jedno żądanie na połączenie (`Connection: close`).

## Testy
`tests/std/*.zsharp` + `*.out` (złote wyniki z interpretera, powtarzane na backendzie C: `scripts/tests/run_all.zsharp --native --filter=tests/std/`),
`tests/std/net_server.zsharp` + `net_client.zsharp` (pętla zwrotna: serwer w tle, klient porównywany z `net_client.out`).
