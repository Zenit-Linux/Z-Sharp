export type Health = "done" | "partial" | "planned";

export interface Milestone { id: string; title: string; health: Health; done: string[]; todo: string[]; unlocks: string; }

export const MILESTONES: readonly Milestone[] = [
  { id: "M1", title: "Moduły i linker", health: "done", unlocks: "podział na pliki, biblioteki funkcji",
    done: ["lib/link.zsharp: funkcje i klasy dla C, LLVM, zst"], todo: ["kod top-level w module"] },
  { id: "M2", title: "Klasy i wyjątki", health: "done", unlocks: "duże narzędzia natywnie (stage3)",
    done: ["lowering klas do funkcji + obiekty w runtime", "klasy wyjątków", "stage3: zsc kompiluje zsc"], todo: ["egzekwowanie private/abstract"] },
  { id: "M3", title: "Idiomatyczny kod", health: "done", unlocks: "callbacki, kod w stylu PHP",
    done: ["domknięcia i funkcje strzałkowe", "argumenty nazwane, variadic, spread w wywołaniach", "spread w literale tablicy [...$a]", "pola statyczne", "map/filter/reduce/each/find/any/all/sum/usort"],
    todo: ["domknięcia w zst js (brak runtime/zs_runtime.js w repozytorium)"] },
  { id: "M4", title: "Zwalnianie pamięci", health: "done", unlocks: "długo żyjące procesy, serwery",
    done: ["ZAMM: areny per ramka (chained arenas), bez GC", "promocja ucieczek: return, bariera zapisu, statyki, wyjątki", "regiony pętli z analizą żywotności", "ZS_MEM_STATS, testy obciążeniowe"],
    todo: ["kompaktowanie dużych aren", "analiza ucieczki w SSA (mniej kopiowania)"] },
  { id: "M5", title: "Biblioteka standardowa", health: "partial", unlocks: "narzędzia „na serio”",
    done: ["json, toml, regex, time, path, os, net", "prymitywy System/File/Socket w stage0 i runtime C", "import Std.* wbudowany w stage0"],
    todo: ["HTTPS natywnie (TLS)", "regex: pełny Unicode"] },
  { id: "M6", title: "VM bajtkodowa i JIT", health: "partial", unlocks: "wydajny REPL i skrypty", done: ["bytecode VM w stage0 (leniwa, z fallbackiem)", "zsi --jit: kompilacja natywna z cache"], todo: ["szybsza reprezentacja wartości w VM", "JIT in-process"] },
];

export interface StdModule { name: string; file: string; summary: string; functions: readonly string[]; }

export const STDLIB: readonly StdModule[] = [
  { name: "Std.Json", file: "stdlib/json.zsharp", summary: "JSON (RFC 8259)", functions: ["decode", "encode", "encodePretty", "valid"] },
  { name: "Std.Toml", file: "stdlib/toml.zsharp", summary: "TOML 1.0", functions: ["decode", "encode"] },
  { name: "Std.Regex", file: "stdlib/regex.zsharp", summary: "wyrażenia regularne (silnik z cofaniem)", functions: ["compile", "test", "fullMatch", "find", "findAll", "replace", "split", "escape"] },
  { name: "Std.Time", file: "stdlib/time.zsharp", summary: "zegar, kalendarz, ISO 8601, strftime", functions: ["nowSeconds", "nowMillis", "nowNanos", "monotonic", "sleep", "localOffset", "make", "fromUnix", "toUnix", "local", "weekday", "yearDay", "isLeap", "daysInMonth", "add", "addDays", "addMonths", "diff", "compare", "startOfDay", "toIso", "parseIso", "format", "parse", "formatDuration", "parseDuration"] },
  { name: "Std.Path", file: "stdlib/path.zsharp", summary: "ścieżki POSIX", functions: ["isAbs", "parts", "normalize", "join", "dirname", "basename", "ext", "stem", "withExt", "split", "relative", "absolute", "isSubpath", "expandHome"] },
  { name: "Std.Os", file: "stdlib/os.zsharp", summary: "środowisko, pliki, procesy", functions: ["platform", "arch", "cpus", "hostname", "pid", "args", "env", "setEnv", "unsetEnv", "environ", "cwd", "chdir", "home", "tempDir", "exit", "stat", "isFile", "isDir", "exists", "size", "readFile", "readLines", "writeFile", "appendFile", "mkdir", "remove", "removeAll", "rename", "copy", "list", "walk", "realpath", "chmod", "symlink", "readlink", "mkdtemp", "run", "exec", "shell", "which"] },
  { name: "Std.Net", file: "stdlib/net.zsharp", summary: "TCP/UDP/DNS, URL, klient i serwer HTTP/1.1", functions: ["connect", "listen", "accept", "read", "readAll", "write", "close", "setTimeout", "localAddr", "peerAddr", "udpBind", "sendTo", "recvFrom", "resolve", "parseUrl", "urlEncode", "urlDecode", "buildQuery", "parseQuery", "request", "get", "post", "put", "delete", "serve", "readRequest", "response"] },
];

export interface Example { id: string; title: string; description: string; code: string; }

export const EXAMPLES: readonly Example[] = [
  { id: "hello", title: "Witaj, Z#", description: "Zmienne, tablice, pętla foreach.",
    code: `import System;

func main(): void {
    $name = "Z#";
    $numbers = [10, 20, 30];
    $sum = 0;
    foreach ($numbers as $n) { $sum += $n; } # suma
    echo "Hello from " . $name . " " . $sum;
}

main();` },
  { id: "closures", title: "Domknięcia i spread", description: "Funkcje strzałkowe, reduce i rozwinięcie tablic [...$a] (M3).",
    code: `$a = [1, 2, 3];
$b = [...$a, 4, ...[5, 6]];
$double = fn($x) => $x * 2;
echo map($b, $double);
echo reduce($b, fn($acc, $x) => $acc + $x, 0);` },
  { id: "classes", title: "Klasy i wyjątki", description: "Dziedziczenie, konstruktor, wyjątki.",
    code: `class Shape {
    public $name;
    public func __construct($name) { $this->name = $name; }
    public func area() { throw new Exception("abstract"); }
}
class Square extends Shape {
    public $side;
    public func __construct($side) { parent.__construct("square"); $this->side = $side; }
    public func area() { return $this->side * $this->side; }
}
try { echo (new Square(4))->area(); }
catch (Exception $e) { echo $e->getMessage(); }` },
  { id: "stdlib", title: "Biblioteka standardowa", description: "JSON i czas z Std.*.",
    code: `import Std.Json;
import Std.Time;

$doc = Json.decode("{\\"name\\": \\"Z#\\", \\"tags\\": [1, 2]}");
echo $doc["name"], count($doc["tags"]);
echo Time.toIso(Time.fromUnix(0));` },
  { id: "memory", title: "Pamięć (M4)", description: "Długo żyjący proces nie rośnie — areny ZAMM zwalniają pamięć z ramką i iteracją pętli, bez GC.",
    code: `for ($i = 0; $i < 1000000; $i++) {
    $tmp = [$i, "item-" . $i];   # śmieci
}
echo "gotowe";  # ZS_MEM_STATS=1 pokazuje raport aren` },
];

export interface Doc { id: string; title: string; file: string; summary: string; }

export const DOCS: readonly Doc[] = [
  { id: "spec", title: "Specyfikacja języka", file: "SPEC.md", summary: "składnia, typy, moduły, pamięć" },
  { id: "stdlib", title: "Biblioteka standardowa", file: "STDLIB.md", summary: "Std.* i prymitywy systemowe (M5)" },
  { id: "status", title: "Status i mapa drogowa", file: "STATUS.md", summary: "narzędzia, bootstrap, kamienie milowe" },
  { id: "memory", title: "Zarządzanie pamięcią", file: "MEMORY.md", summary: "ZAMM — chained arenas (M4)" },
  { id: "vm", title: "VM i JIT", file: "VM.md", summary: "bajtkod, zsi --jit (M6)" },
  { id: "bootstrap", title: "Bootstrap", file: "BOOTSTRAP.md", summary: "od ziarna Rust do stage3" },
  { id: "ast", title: "Format AST", file: "AST.md", summary: "kanoniczny zapis drzewa" },
  { id: "changelog", title: "Historia zmian", file: "CHANGELOG.md", summary: "co się zmieniło" },
];
