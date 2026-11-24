# Biblioteka standardowa Z# (`Std.*`) — M5

Minimalistyczna: tylko to, czego potrzebują narzędzia i skrypty. Dwanaście modułów, zero zależności zewnętrznych.
Import: `import Std.Json;` — nazwa modułu w kodzie to ostatni segment (`Json.decode(...)`).

| Moduł | Plik | Zakres |
|---|---|---|
| `Std.Json` | `stdlib/json.zsharp` | JSON (RFC 8259): `decode`, `encode`, `encodePretty`, `valid` |
| `Std.Toml` | `stdlib/toml.zsharp` | TOML 1.0: `decode`, `encode` |
| `Std.Regex` | `stdlib/regex.zsharp` | wyrażenia regularne: `compile`, `test`, `fullMatch`, `find`, `findAll`, `replace`, `split`, `escape` |
| `Std.Time` | `stdlib/time.zsharp` | zegar, kalendarz, ISO 8601, `strftime`/`strptime`, czasy trwania |
| `Std.Path` | `stdlib/path.zsharp` | ścieżki POSIX (czysty tekst) |
| `Std.Os` | `stdlib/os.zsharp` | system: środowisko, pliki, procesy, `which` |
| `Std.Net` | `stdlib/net.zsharp` | TCP/UDP/DNS, URL, klient i serwer HTTP/1.1 (HTTPS przez `curl`) |
| `Std.Base64` | `stdlib/base64.zsharp` | Base64 (standardowy i URL-safe), bajty, hex |
| `Std.Hash` | `stdlib/hash.zsharp` | SHA-256, SHA-1, MD5, HMAC-SHA256, CRC-32, Adler-32, FNV-1a |
| `Std.Csv` | `stdlib/csv.zsharp` | CSV (RFC 4180): `parse`, `parseHeader`, `encode`, `encodeHeader` |
| `Std.Random` | `stdlib/random.zsharp` | deterministyczny generator `Rng` (xorshift128), `uuid4`, `shuffle`, `sample` |
| `Std.Text` | `stdlib/text.zsharp` | `format` (interpolacja `{klucz}`), wielkość liter, `slug`, `wrap`, `truncate`, `levenshtein` … |

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
* **HTTPS przez zewnętrzne `curl`** (musi być w PATH; weryfikacja certyfikatu i proxy ze środowiska; treść żądania ≤ 100 kB). Własnego TLS brak.
* Dane są tekstem UTF-8; treści binarne nie są bezpieczne (nieprawidłowe sekwencje zamieniane na U+FFFD). `Socket.read` dobiera brakujące bajty
  niepełnego znaku na końcu porcji, więc wielobajtowe znaki nie są uszkadzane na granicy odczytu.
* Serwer jest jednowątkowy i obsługuje jedno żądanie na połączenie (`Connection: close`).

## Testy
`tests/std/*.zsharp` + `*.out` (złote wyniki z interpretera, powtarzane na backendzie C: `scripts/tests/run_all.zsharp --native --filter=tests/std/`),
`tests/std/net_server.zsharp` + `net_client.zsharp` (pętla zwrotna: serwer w tle, klient porównywany z `net_client.out`).

## Dodatki (M5, sesja „M3/M4/M5 domknięte")
* `Std.Time`: strefy z regułami DST (EU/US/AU/NZ): `zoneInfo, zoneOffset, zoneAbbr, isDst, inZone, makeInZone, addInZone`; `%Z` w formatowaniu.
* `Std.Regex`: lookbehind `(?<=..)`/`(?<!..)`, `\p{L}`, `\P{..}`, flaga `u` (tablice Unicode przybliżone).
* `System.gc()` (pusta, zgodność) / `System.gcStats()` (liczniki ZAMM) — patrz `docs/MEMORY.md`.

## Std.Base64
`encode($tekst)`, `decode($b64)`, `encodeUrl/decodeUrl` (alfabet `-_`, bez `=`), `encodeBytes($bajty)`, `decodeBytes($b64)` → tablica 0..255, `valid`,
`hexEncode($tekst|$bajty)`, `hexDecode($hex)` → bajty. `decode` akceptuje oba alfabety i ignoruje białe znaki.

## Std.Hash
`sha256`, `sha1`, `md5`, `hmacSha256($klucz, $dane)` → hex; wersje `*Bytes` → tablica bajtów. `crc32($dane, $start = 0)`, `adler32`, `fnv1a` → int.
Argument to tekst (bajty UTF-8) lub tablica bajtów. Wartości sprawdzone względem `hashlib`/`zlib`/`hmac` Pythona. Czysta arytmetyka 32-bitowa
(int w Z# ma kontrolę przepełnienia, dlatego każdy krok jest maskowany). MD5/SHA-1 tylko dla zgodności.

## Std.Csv
`parse($tekst, $sep = ",")` → lista wierszy; `parseHeader` → lista tablic asocjacyjnych; `encode($wiersze, $sep)`; `encodeHeader($rekordy, $kolumny = null, $sep)`.
Cudzysłowy, `""`, nowe linie w polach, `\r\n`, BOM. Pola zawsze jako teksty; błędy (niezamknięty cudzysłów) → `Exception` z linią.

## Std.Random
`$g = new Rng($ziarno = null)` (po `import Std.Random;`): `nextInt, int($min,$max), float, bool, pick, shuffle, sample, bytes, hex, uuid4`.
Ta sama wartość ziarna daje tę samą sekwencję w interpreterze i w kodzie natywnym. **Nie** do celów kryptograficznych.

## Std.Text
`format($szablon, $dane)` — interpolacja `{imie}`, `{a.b.0}`, `{0}`, `{{`/`}}` (brak klucza → wyjątek). `lines, words, isBlank, capitalize, title, snake, kebab, camel, slug,
length` (znaki Unicode), `truncate, wrap, indent, dedent, center, countOf, commonPrefix, levenshtein, reverseText`.
Uwaga: `substr` działa na bajtach, więc moduł iteruje po `chars()`, gdy tekst może zawierać znaki spoza ASCII.
