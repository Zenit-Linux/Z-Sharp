use std::process::ExitCode;
use zs_lexer::Lexer;

const USAGE: &str = "użycie:\n  zsc0 run <plik.zs> [args...]\n  zsc0 exec <plik.zbc> [args...]\n  zsc0 check <plik.zs>\n  zsc0 --tokens <plik.zs>\n  zsc0 --ast <plik.zs>\n  zsc0 --sexp <plik.zs>\n  zsc0 manifest [katalog]\n  zsc0 --version";

fn read(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| { eprintln!("{path}: {e}"); ExitCode::from(1) })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { eprintln!("{USAGE}"); return ExitCode::from(2) };
    match (cmd.as_str(), args.get(1)) {
        ("--version", _) => { println!("zsc0 {}", env!("CARGO_PKG_VERSION")); ExitCode::SUCCESS }
        ("run", Some(path)) => {
            let code = zs_interp::run_file(path, args[1..].to_vec());
            ExitCode::from(code.clamp(0, 255) as u8)
        }
        ("exec", Some(path)) => {
            let text = match read(path) { Ok(s) => s, Err(c) => return c };
            let code = zs_interp::run_zbc(text, args[1..].to_vec());
            ExitCode::from(code.clamp(0, 255) as u8)
        }
        ("check", Some(path)) => {
            let src = match read(path) { Ok(s) => s, Err(c) => return c };
            match zs_parser::parse(&src) { Ok(_) => ExitCode::SUCCESS, Err(e) => { eprintln!("{path}:{e}"); ExitCode::from(1) } }
        }
        ("--tokens", Some(path)) => {
            let src = match read(path) { Ok(s) => s, Err(c) => return c };
            match Lexer::new(&src).tokenize() {
                Ok(toks) => { for t in toks { println!("{:>4}:{:<3} {:?}", t.span.line, t.span.col, t.kind); } ExitCode::SUCCESS }
                Err(e) => { eprintln!("{path}:{e}"); ExitCode::from(1) }
            }
        }
        ("--ast", Some(path)) => {
            let src = match read(path) { Ok(s) => s, Err(c) => return c };
            match zs_parser::parse(&src) { Ok(p) => { println!("{p:#?}"); ExitCode::SUCCESS } Err(e) => { eprintln!("{path}:{e}"); ExitCode::from(1) } }
        }
        ("--sexp", Some(path)) => {
            let src = match read(path) { Ok(s) => s, Err(c) => return c };
            match zs_parser::parse(&src) { Ok(p) => { for l in zs_parser::dump::stmts(&p) { println!("{l}"); } ExitCode::SUCCESS } Err(e) => { eprintln!("{path}:{e}"); ExitCode::from(1) } }
        }
        ("manifest", dir) => manifest_cmd(dir.map(|s| s.as_str()).unwrap_or(".")),
        _ => { eprintln!("{USAGE}"); ExitCode::from(2) }
    }
}

fn manifest_cmd(dir: &str) -> ExitCode {
    let start = std::path::Path::new(dir).canonicalize().unwrap_or_else(|_| dir.into());
    let Some(root) = zs_manifest::find_manifest(&start) else { eprintln!("brak {} w {} ani w katalogach nadrzędnych", zs_manifest::MANIFEST_NAME, dir); return ExitCode::from(1) };
    let m = match zs_manifest::Manifest::load(&root) { Ok(m) => m, Err(e) => { eprintln!("{e}"); return ExitCode::from(1) } };
    println!("manifest: {}", root.join(zs_manifest::MANIFEST_NAME).display());
    println!("pakiet:   {} {} ({:?}){}", if m.name.is_empty() { "-" } else { &m.name }, m.version, m.kind, m.entry.as_ref().map(|e| format!(", entry={e}")).unwrap_or_default());
    let mut bad = 0;
    for (name, p) in &m.members {
        let ok = m.dir.join(p).is_file();
        if !ok { bad += 1; }
        println!("  Workspace.{:<10} {} {}", name, p, if ok { "ok" } else { "BRAK PLIKU" });
    }
    for d in &m.dependencies {
        let found = d.path.as_ref().map(|p| m.dir.join(p).join(zs_manifest::MANIFEST_NAME).is_file());
        println!("  dep {:<12} path={} git={} {}", d.name, d.path.as_deref().unwrap_or("-"), d.git.as_deref().unwrap_or("-"),
            match found { Some(true) => "ok", Some(false) => "(katalog zależności nieobecny)", None => "" });
    }
    for x in &m.externs { println!("  extern {} ({}) {}", x.name, x.kind, x.path.as_deref().unwrap_or("")); }
    if !m.memory.is_empty() { println!("  [memory] {}", m.memory.iter().map(|(k, v)| format!("{k}={}", zs_manifest::dump(v, 0))).collect::<Vec<_>>().join(", ")); }
    if bad > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
