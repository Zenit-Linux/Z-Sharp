use std::process::ExitCode;
use zs_lexer::Lexer;

const USAGE: &str = "użycie:\n  zsc0 run <plik.zs> [args...]\n  zsc0 check <plik.zs>\n  zsc0 --tokens <plik.zs>\n  zsc0 --ast <plik.zs>\n  zsc0 --version";

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
        _ => { eprintln!("{USAGE}"); ExitCode::from(2) }
    }
}
