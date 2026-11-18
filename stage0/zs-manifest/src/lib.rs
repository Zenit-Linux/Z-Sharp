use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub const MANIFEST_NAME: &str = "Z#.toml";
/// Rozszerzenia plików źródłowych, w kolejności próbowania przy rozwiązywaniu importów.
pub const SOURCE_EXTS: &[&str] = &["zsharp", "zs"];

// ───────────────────────── wartości TOML ─────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<Value>),
    Table(Table),
}
pub type Table = BTreeMap<String, Value>;

impl Value {
    pub fn as_str(&self) -> Option<&str> { if let Value::Str(s) = self { Some(s) } else { None } }
    pub fn as_int(&self) -> Option<i64> { if let Value::Int(i) = self { Some(*i) } else { None } }
    pub fn as_bool(&self) -> Option<bool> { if let Value::Bool(b) = self { Some(*b) } else { None } }
    pub fn as_array(&self) -> Option<&[Value]> { if let Value::Array(a) = self { Some(a) } else { None } }
    pub fn as_table(&self) -> Option<&Table> { if let Value::Table(t) = self { Some(t) } else { None } }
    pub fn type_name(&self) -> &'static str {
        match self { Value::Str(_) => "string", Value::Int(_) => "int", Value::Float(_) => "float",
            Value::Bool(_) => "bool", Value::Array(_) => "array", Value::Table(_) => "table" }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError { pub line: usize, pub msg: String }
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}: {}", self.line, self.msg) }
}
impl std::error::Error for ParseError {}

fn err<T>(line: usize, msg: impl Into<String>) -> Result<T, ParseError> { Err(ParseError { line, msg: msg.into() }) }

struct Cur<'a> { s: &'a [u8], i: usize, line: usize }

impl<'a> Cur<'a> {
    fn peek(&self) -> Option<u8> { self.s.get(self.i).copied() }
    fn ws(&mut self) { while matches!(self.peek(), Some(b' ' | b'\t')) { self.i += 1; } }
    fn ws_nl(&mut self) { // spacje, nowe linie i komentarze (wewnątrz tablic)
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => self.i += 1,
                Some(b'\n') => { self.i += 1; self.line += 1; }
                Some(b'#') => { while !matches!(self.peek(), None | Some(b'\n')) { self.i += 1; } }
                _ => break,
            }
        }
    }
    fn eat(&mut self, c: u8) -> bool { if self.peek() == Some(c) { self.i += 1; true } else { false } }
    fn rest_is_blank(&mut self) -> bool {
        self.ws();
        if self.peek() == Some(b'#') { while !matches!(self.peek(), None | Some(b'\n')) { self.i += 1; } }
        matches!(self.peek(), None | Some(b'\n') | Some(b'\r'))
    }
}

fn is_bare(c: u8) -> bool { c.is_ascii_alphanumeric() || c == b'_' || c == b'-' }

fn parse_string(c: &mut Cur) -> Result<String, ParseError> {
    let q = c.peek().unwrap();
    c.i += 1;
    let mut out: Vec<u8> = vec![];
    loop {
        match c.peek() {
            None | Some(b'\n') => return err(c.line, "niezamknięty napis"),
            Some(ch) if ch == q => { c.i += 1; break; }
            Some(b'\\') if q == b'"' => {
                c.i += 1;
                let e = c.peek().ok_or(ParseError { line: c.line, msg: "ucięta sekwencja ucieczki".into() })?;
                c.i += 1;
                match e {
                    b'n' => out.push(b'\n'), b't' => out.push(b'\t'), b'r' => out.push(b'\r'),
                    b'"' => out.push(b'"'), b'\\' => out.push(b'\\'), b'0' => out.push(0),
                    b'u' | b'U' => {
                        let n = if e == b'u' { 4 } else { 8 };
                        let h = std::str::from_utf8(&c.s[c.i..(c.i + n).min(c.s.len())]).unwrap_or("");
                        let cp = u32::from_str_radix(h, 16).ok().and_then(char::from_u32);
                        match cp { Some(ch) => { let mut b = [0u8; 4]; out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes()); c.i += n; }
                                   None => return err(c.line, "niepoprawna sekwencja \\u") }
                    }
                    _ => return err(c.line, format!("nieznana sekwencja ucieczki \\{}", e as char)),
                }
            }
            Some(ch) => { out.push(ch); c.i += 1; }
        }
    }
    String::from_utf8(out).map_err(|_| ParseError { line: c.line, msg: "napis nie jest UTF-8".into() })
}

fn parse_key_part(c: &mut Cur) -> Result<String, ParseError> {
    c.ws();
    match c.peek() {
        Some(b'"' | b'\'') => parse_string(c),
        Some(ch) if is_bare(ch) => {
            let st = c.i;
            while c.peek().map_or(false, is_bare) { c.i += 1; }
            Ok(String::from_utf8_lossy(&c.s[st..c.i]).into_owned())
        }
        _ => err(c.line, "oczekiwano klucza"),
    }
}

fn parse_key_path(c: &mut Cur) -> Result<Vec<String>, ParseError> {
    let mut p = vec![parse_key_part(c)?];
    loop { c.ws(); if c.eat(b'.') { p.push(parse_key_part(c)?); } else { break; } }
    Ok(p)
}

fn parse_value(c: &mut Cur) -> Result<Value, ParseError> {
    c.ws();
    match c.peek() {
        None => err(c.line, "brak wartości"),
        Some(b'"' | b'\'') => {
            if c.s[c.i..].starts_with(b"\"\"\"") || c.s[c.i..].starts_with(b"'''") { return err(c.line, "napisy wieloliniowe nie są wspierane w Z#.toml"); }
            Ok(Value::Str(parse_string(c)?))
        }
        Some(b'[') => {
            c.i += 1;
            let mut v = vec![];
            loop {
                c.ws_nl();
                if c.eat(b']') { break; }
                v.push(parse_value(c)?);
                c.ws_nl();
                if c.eat(b',') { continue; }
                c.ws_nl();
                if c.eat(b']') { break; }
                return err(c.line, "oczekiwano ',' lub ']' w tablicy");
            }
            Ok(Value::Array(v))
        }
        Some(b'{') => {
            c.i += 1;
            let mut t = Table::new();
            c.ws();
            if c.eat(b'}') { return Ok(Value::Table(t)); }
            loop {
                let path = parse_key_path(c)?;
                c.ws();
                if !c.eat(b'=') { return err(c.line, "oczekiwano '='"); }
                let v = parse_value(c)?;
                insert_path(&mut t, &path, v, c.line)?;
                c.ws();
                if c.eat(b',') { continue; }
                if c.eat(b'}') { break; }
                return err(c.line, "oczekiwano ',' lub '}' w tabeli inline");
            }
            Ok(Value::Table(t))
        }
        Some(_) => {
            let st = c.i;
            while let Some(ch) = c.peek() { if matches!(ch, b',' | b']' | b'}' | b'#' | b'\n' | b'\r' | b' ' | b'\t') { break; } c.i += 1; }
            let tok = std::str::from_utf8(&c.s[st..c.i]).unwrap_or("");
            match tok {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "" => err(c.line, "brak wartości"),
                _ => {
                    let t = tok.replace('_', "");
                    if let Some(h) = t.strip_prefix("0x") { return i64::from_str_radix(h, 16).map(Value::Int).or_else(|_| err(c.line, "zła liczba hex")); }
                    if let Ok(i) = t.parse::<i64>() { return Ok(Value::Int(i)); }
                    if let Ok(f) = t.parse::<f64>() { return Ok(Value::Float(f)); }
                    err(c.line, format!("nierozpoznana wartość `{tok}`"))
                }
            }
        }
    }
}

fn insert_path(t: &mut Table, path: &[String], v: Value, line: usize) -> Result<(), ParseError> {
    if path.len() == 1 {
        if t.contains_key(&path[0]) { return err(line, format!("zduplikowany klucz `{}`", path[0])); }
        t.insert(path[0].clone(), v);
        return Ok(());
    }
    match t.entry(path[0].clone()).or_insert_with(|| Value::Table(Table::new())) {
        Value::Table(sub) => insert_path(sub, &path[1..], v, line),
        _ => err(line, format!("`{}` nie jest tabelą", path[0])),
    }
}

/// Zwraca tabelę, do której trzeba pisać dla nagłówka `[a.b]` / `[[a.b]]`.
fn open_table<'t>(root: &'t mut Table, path: &[String], array: bool, line: usize) -> Result<&'t mut Table, ParseError> {
    let mut cur = root;
    for (i, seg) in path.iter().enumerate() {
        let last = i + 1 == path.len();
        if last && array {
            let e = cur.entry(seg.clone()).or_insert_with(|| Value::Array(vec![]));
            match e {
                Value::Array(a) => { a.push(Value::Table(Table::new())); match a.last_mut() { Some(Value::Table(t)) => return Ok(t), _ => unreachable!() } }
                _ => return err(line, format!("`{seg}` nie jest tablicą tabel")),
            }
        }
        let e = cur.entry(seg.clone()).or_insert_with(|| Value::Table(Table::new()));
        cur = match e {
            Value::Table(t) => t,
            Value::Array(a) => match a.last_mut() { Some(Value::Table(t)) => t, _ => return err(line, format!("`{seg}` nie jest tabelą")) },
            _ => return err(line, format!("`{seg}` nie jest tabelą")),
        };
    }
    Ok(cur)
}

/// Parsuje tekst TOML do tabeli głównej.
pub fn parse_toml(text: &str) -> Result<Table, ParseError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut c = Cur { s: text.as_bytes(), i: 0, line: 1 };
    let mut root = Table::new();
    let mut cur_path: Vec<String> = vec![];
    let mut cur_is_arr = false;
    let mut fresh_header = false;
    loop {
        c.ws_nl();
        if c.peek().is_none() { break; }
        if c.peek() == Some(b'[') {
            c.i += 1;
            let arr = c.eat(b'[');
            let path = parse_key_path(&mut c)?;
            c.ws();
            if !c.eat(b']') { return err(c.line, "oczekiwano ']'"); }
            if arr && !c.eat(b']') { return err(c.line, "oczekiwano ']]'"); }
            if !c.rest_is_blank() { return err(c.line, "śmieci po nagłówku tabeli"); }
            cur_path = path; cur_is_arr = arr; fresh_header = true;
            if arr { open_table(&mut root, &cur_path, true, c.line)?; fresh_header = false; }
            else {
                // `[a.b]` zdefiniowane dwa razy to błąd
                let t = open_table(&mut root, &cur_path, false, c.line)?;
                let _ = t;
            }
            continue;
        }
        let _ = (cur_is_arr, fresh_header);
        let path = parse_key_path(&mut c)?;
        c.ws();
        if !c.eat(b'=') { return err(c.line, "oczekiwano '='"); }
        let line = c.line;
        let v = parse_value(&mut c)?;
        if !c.rest_is_blank() { return err(c.line, "śmieci po wartości"); }
        let t = if cur_path.is_empty() { &mut root } else {
            // dla [[arr]] piszemy do ostatniego elementu, dla [t] do tabeli
            open_table_nopush(&mut root, &cur_path, line)?
        };
        insert_path(t, &path, v, line)?;
    }
    Ok(root)
}

fn open_table_nopush<'t>(root: &'t mut Table, path: &[String], line: usize) -> Result<&'t mut Table, ParseError> {
    let mut cur = root;
    for seg in path {
        let e = cur.entry(seg.clone()).or_insert_with(|| Value::Table(Table::new()));
        cur = match e {
            Value::Table(t) => t,
            Value::Array(a) => match a.last_mut() { Some(Value::Table(t)) => t, _ => return err(line, format!("`{seg}` nie jest tabelą")) },
            _ => return err(line, format!("`{seg}` nie jest tabelą")),
        };
    }
    Ok(cur)
}

/// Serializuje wartość do tekstu zbliżonego do Z# (do podglądu / `zsc0 manifest --dump`).
pub fn dump(v: &Value, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match v {
        Value::Str(s) => format!("{s:?}"), Value::Int(i) => i.to_string(), Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(a) => format!("[{}]", a.iter().map(|x| dump(x, indent)).collect::<Vec<_>>().join(", ")),
        Value::Table(t) => {
            let mut s = String::from("{\n");
            for (k, x) in t { s += &format!("{pad}  {k} = {}\n", dump(x, indent + 1)); }
            s + &pad + "}"
        }
    }
}

// ───────────────────────── model manifestu ─────────────────────────
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind { Bin, Lib }

#[derive(Debug, Clone)]
pub struct Dependency { pub name: String, pub path: Option<String>, pub git: Option<String>, pub version: Option<String>, pub rev: Option<String> }

#[derive(Debug, Clone)]
pub struct ExternLib { pub name: String, pub kind: String, pub path: Option<String>, pub headers: Vec<String>, pub link_args: Vec<String> }

#[derive(Debug, Clone)]
pub struct Manifest {
    pub dir: PathBuf,
    pub name: String,
    pub version: String,
    pub kind: Kind,
    pub entry: Option<String>,
    pub description: String,
    pub license: String,
    /// `[workspace.members]`: nazwa -> ścieżka pliku względem tego manifestu
    pub members: BTreeMap<String, String>,
    pub dependencies: Vec<Dependency>,
    pub externs: Vec<ExternLib>,
    /// `[memory]` — konfiguracja ZAMM (klucz -> wartość)
    pub memory: Table,
    pub raw: Table,
}

#[derive(Debug)]
pub struct ManifestError(pub String);
impl fmt::Display for ManifestError { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) } }
impl std::error::Error for ManifestError {}

fn get_str(t: &Table, k: &str) -> Option<String> { t.get(k).and_then(|v| v.as_str()).map(|s| s.to_string()) }

fn valid_ident(s: &str) -> bool {
    let mut ch = s.chars();
    matches!(ch.next(), Some(c) if c.is_ascii_alphabetic() || c == '_') && ch.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Manifest {
    pub fn parse(text: &str, dir: &Path) -> Result<Manifest, ManifestError> {
        let raw = parse_toml(text).map_err(|e| ManifestError(format!("{MANIFEST_NAME}:{e}")))?;
        let e = |m: String| ManifestError(format!("{MANIFEST_NAME}: {m}"));
        let empty = Table::new();
        let pkg = raw.get("package").and_then(|v| v.as_table());
        let ws = raw.get("workspace").and_then(|v| v.as_table());
        if pkg.is_none() && ws.is_none() { return Err(e("wymagana sekcja [package] lub [workspace]".into())); }
        let pkg = pkg.unwrap_or(&empty);
        let name = get_str(pkg, "name").unwrap_or_default();
        if pkg.get("name").is_some() && name.is_empty() { return Err(e("[package].name nie może być pusty".into())); }
        let kind = match get_str(pkg, "kind").as_deref() {
            None | Some("bin") => Kind::Bin, Some("lib") => Kind::Lib,
            Some(o) => return Err(e(format!("[package].kind: `{o}` (dozwolone: bin, lib)"))),
        };
        let version = get_str(pkg, "version").unwrap_or_else(|| "0.0.0".into());
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.parse::<u32>().is_err()) { return Err(e(format!("[package].version `{version}` nie jest w formie MAJOR.MINOR.PATCH"))); }
        let mut members = BTreeMap::new();
        if let Some(m) = ws.and_then(|w| w.get("members")) {
            let Value::Table(m) = m else { return Err(e("[workspace.members] musi być tabelą nazwa = \"ścieżka\"".into())) };
            for (k, v) in m {
                if !valid_ident(k) { return Err(e(format!("człon workspace'u `{k}` nie jest poprawnym identyfikatorem Z#"))); }
                let Some(p) = v.as_str() else { return Err(e(format!("człon `{k}`: ścieżka musi być napisem"))) };
                members.insert(k.clone(), p.to_string());
            }
        }
        let mut dependencies = vec![];
        if let Some(Value::Table(d)) = raw.get("dependencies") {
            for (k, v) in d {
                match v {
                    Value::Str(ver) => dependencies.push(Dependency { name: k.clone(), path: None, git: None, version: Some(ver.clone()), rev: None }),
                    Value::Table(t) => dependencies.push(Dependency { name: k.clone(), path: get_str(t, "path"), git: get_str(t, "git"), version: get_str(t, "version"), rev: get_str(t, "rev") }),
                    o => return Err(e(format!("[dependencies].{k}: oczekiwano napisu lub tabeli, jest {}", o.type_name()))),
                }
            }
        }
        let mut externs = vec![];
        if let Some(Value::Table(x)) = raw.get("extern") {
            for (k, v) in x {
                let Value::Table(t) = v else { return Err(e(format!("[extern.{k}] musi być tabelą"))) };
                let kind = get_str(t, "kind").unwrap_or_else(|| "static".into());
                if !matches!(kind.as_str(), "static" | "dylib" | "framework") { return Err(e(format!("[extern.{k}].kind `{kind}` (static|dylib)"))); }
                let list = |key: &str| t.get(key).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                externs.push(ExternLib { name: k.clone(), kind, path: get_str(t, "path"), headers: list("headers"), link_args: list("link-args") });
            }
        }
        let memory = match raw.get("memory") { Some(Value::Table(t)) => t.clone(), _ => Table::new() };
        if kind == Kind::Bin && pkg.get("name").is_some() && get_str(pkg, "entry").is_none() { return Err(e("pakiet typu bin wymaga [package].entry".into())); }
        Ok(Manifest { dir: dir.to_path_buf(), name, version, kind, entry: get_str(pkg, "entry"), description: get_str(pkg, "description").unwrap_or_default(),
            license: get_str(pkg, "license").unwrap_or_default(), members, dependencies, externs, memory, raw })
    }

    pub fn load(dir: &Path) -> Result<Manifest, ManifestError> {
        let f = dir.join(MANIFEST_NAME);
        let text = std::fs::read_to_string(&f).map_err(|e| ManifestError(format!("{}: {e}", f.display())))?;
        let mut m = Manifest::parse(&text, dir)?;
        if m.dir.as_os_str().is_empty() { m.dir = PathBuf::from("."); }
        Ok(m)
    }

    /// Ścieżka pliku członka workspace'u (bezwzględna względem katalogu manifestu).
    pub fn member_path(&self, name: &str) -> Option<PathBuf> { self.members.get(name).map(|p| self.dir.join(p)) }
}

/// Znajduje najbliższy `Z#.toml` idąc w górę od `start`.
pub fn find_manifest(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|d| d.join(MANIFEST_NAME).is_file()).map(|d| d.to_path_buf())
}

/// Korzenie, w których szukamy workspace'u: `ZS_WORKSPACE`, katalogi nadrzędne (i ich `Z-Sharp/`),
/// oraz zależności typu `path` najbliższego manifestu. Zwraca pierwszy manifest z `[workspace.members]`.
pub fn find_workspace(start: &Path) -> Option<Manifest> {
    let mut roots: Vec<PathBuf> = vec![];
    if let Ok(r) = std::env::var("ZS_WORKSPACE") { roots.push(PathBuf::from(r)); }
    if let Some(local) = find_manifest(start) {
        roots.push(local.clone());
        if let Ok(m) = Manifest::load(&local) {
            for d in &m.dependencies { if let Some(p) = &d.path { roots.push(local.join(p)); } }
        }
    }
    for a in start.ancestors() { roots.push(a.to_path_buf()); roots.push(a.join("Z-Sharp")); }
    for r in roots {
        if r.join(MANIFEST_NAME).is_file() {
            if let Ok(m) = Manifest::load(&r) { if !m.members.is_empty() { return Some(m); } }
        }
    }
    None
}

/// Rozwiązuje `import Workspace.<człon>[.<podmoduł>...]` do pliku.
pub fn resolve_workspace_import(start: &Path, segs: &[&str]) -> Option<PathBuf> {
    let ws = find_workspace(start)?;
    let mut f = ws.member_path(segs.first()?)?;
    for extra in &segs[1..] {
        let dir = f.parent()?.to_path_buf();
        f = with_source_ext(&dir.join(extra))?;
    }
    f.is_file().then_some(f)
}

/// `base` bez rozszerzenia -> istniejący plik `.zsharp` lub `.zs`.
pub fn with_source_ext(base: &Path) -> Option<PathBuf> {
    for e in SOURCE_EXTS {
        let mut s = base.as_os_str().to_os_string(); s.push("."); s.push(e);
        let p = PathBuf::from(s);
        if p.is_file() { return Some(p); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_and_tables() {
        let t = parse_toml("a = 1\nb = \"x\\ny\"\nc = true # k\nd = 1.5\n[s]\nk = 'lit'\n[s.u]\nz = [1, 2,\n 3, # c\n]\n").unwrap();
        assert_eq!(t["a"], Value::Int(1));
        assert_eq!(t["b"], Value::Str("x\ny".into()));
        assert_eq!(t["c"], Value::Bool(true));
        assert_eq!(t["d"], Value::Float(1.5));
        let s = t["s"].as_table().unwrap();
        assert_eq!(s["k"], Value::Str("lit".into()));
        assert_eq!(s["u"].as_table().unwrap()["z"].as_array().unwrap().len(), 3);
    }
    #[test]
    fn inline_and_dotted() {
        let t = parse_toml("a.b.c = 1\nx = { y = 2, z = \"q\" }\n").unwrap();
        assert_eq!(t["a"].as_table().unwrap()["b"].as_table().unwrap()["c"], Value::Int(1));
        assert_eq!(t["x"].as_table().unwrap()["y"], Value::Int(2));
    }
    #[test]
    fn array_of_tables() {
        let t = parse_toml("[[p]]\nn = 1\n[[p]]\nn = 2\n[p.sub]\nq = 3\n").unwrap();
        let a = t["p"].as_array().unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[1].as_table().unwrap()["sub"].as_table().unwrap()["q"], Value::Int(3));
    }
    #[test]
    fn errors_have_lines() {
        let e = parse_toml("a = 1\nb = \n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(parse_toml("a = 1\na = 2\n").is_err());
        assert!(parse_toml("a = \"abc\n").is_err());
        assert!(parse_toml("[t]\nx = 1 y\n").is_err());
    }
    #[test]
    fn manifest_model() {
        let m = Manifest::parse(r#"
[package]
name = "zsc"
version = "0.0.1"
entry = "src/main.zsharp"
[workspace.members]
lexer = "lib/lexer.zsharp"
[dependencies]
Z-Sharp = { path = "Z-Sharp" }
[extern.sqlite]
kind = "static"
path = "vendor/libsqlite3.a"
[memory]
chunk-kib = 64
"#, Path::new(".")).unwrap();
        assert_eq!(m.name, "zsc");
        assert_eq!(m.members["lexer"], "lib/lexer.zsharp");
        assert_eq!(m.dependencies[0].path.as_deref(), Some("Z-Sharp"));
        assert_eq!(m.externs[0].kind, "static");
        assert_eq!(m.memory["chunk-kib"], Value::Int(64));
    }
    #[test]
    fn manifest_validation() {
        assert!(Manifest::parse("[package]\nname=\"a\"\nversion=\"1.0\"\nentry=\"x\"\n", Path::new(".")).is_err());
        assert!(Manifest::parse("[package]\nname=\"a\"\nversion=\"1.0.0\"\n", Path::new(".")).is_err()); // bin bez entry
        assert!(Manifest::parse("[package]\nname=\"a\"\nkind=\"lib\"\nversion=\"1.0.0\"\n", Path::new(".")).is_ok());
        assert!(Manifest::parse("[workspace.members]\n\"bad name\" = \"x\"\n", Path::new(".")).is_err());
        assert!(Manifest::parse("x = 1\n", Path::new(".")).is_err());
    }
}
