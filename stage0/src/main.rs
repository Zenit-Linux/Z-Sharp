use std::process::ExitCode;
use zs_lexer::Lexer;

const USAGE: &str = "użycie:\n  zsc0 run <plik.zs> [args...]\n  zsc0 exec <plik.zbc> [args...]\n  zsc0 check <plik.zs>\n  zsc0 --tokens <plik.zs>\n  zsc0 --ast <plik.zs>\n  zsc0 --sexp <plik.zs>\n  zsc0 manifest [katalog]\n  zsc0 bundle <pakiet.zbc> -o <wyjście>\n  zsc0 --version";

fn read(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| { eprintln!("{path}: {e}"); ExitCode::from(1) })
}

const BUNDLE_MAGIC: &[u8; 16] = b"\0ZS-BUNDLE-v1\0\0\0";

/// Binarka standalone = kopia zsc0 + pakiet .zbc + [len u64 LE] + magic.
/// Ścieżka własnej binarki. Jako PID 1 (przed zamontowaniem /proc) `current_exe` zawodzi — wtedy `argv[0]`.
fn self_exe() -> std::io::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::current_exe() { return Ok(p); }
    match std::env::args().next() {
        Some(a) if std::path::Path::new(&a).is_file() => Ok(std::path::PathBuf::from(a)),
        _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "nie można ustalić ścieżki własnej binarki")),
    }
}

fn embedded_payload() -> Option<String> {
    let exe = self_exe().ok()?;
    let data = std::fs::read(exe).ok()?;
    if data.len() < 24 || &data[data.len() - 16..] != BUNDLE_MAGIC { return None; }
    let n = u64::from_le_bytes(data[data.len() - 24..data.len() - 16].try_into().ok()?) as usize;
    let end = data.len() - 24;
    if n > end { return None; }
    String::from_utf8(data[end - n..end].to_vec()).ok()
}

fn bundle_cmd(args: &[String]) -> ExitCode {
    let mut input: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-o" { out = args.get(i + 1).map(|s| s.as_str()); i += 2; } else { input = Some(args[i].as_str()); i += 1; }
    }
    let (Some(input), Some(out)) = (input, out) else { eprintln!("{USAGE}"); return ExitCode::from(2) };
    let payload = match std::fs::read(input) { Ok(b) => b, Err(e) => { eprintln!("{input}: {e}"); return ExitCode::from(1) } };
    let exe = match self_exe().and_then(std::fs::read) { Ok(b) => b, Err(e) => { eprintln!("bundle: {e}"); return ExitCode::from(1) } };
    // bazą jest sam interpreter (bez ewentualnego wcześniejszego ładunku)
    let base_len = if exe.len() >= 24 && &exe[exe.len() - 16..] == BUNDLE_MAGIC {
        let n = u64::from_le_bytes(exe[exe.len() - 24..exe.len() - 16].try_into().unwrap()) as usize;
        exe.len().saturating_sub(24 + n)
    } else { exe.len() };
    let mut buf = exe[..base_len].to_vec();
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    buf.extend_from_slice(BUNDLE_MAGIC);
    if let Err(e) = std::fs::write(out, &buf) { eprintln!("{out}: {e}"); return ExitCode::from(1); }
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755)); }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    if let Some(text) = embedded_payload() {
        let args: Vec<String> = std::env::args().collect();
        let code = zs_interp::run_zbc(text, args);
        return ExitCode::from(code.clamp(0, 255) as u8);
    }
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
        ("bundle", Some(_)) => bundle_cmd(&args[1..]),
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
