use crate::interp::*;
use crate::value::*;
use std::io::Write;
use std::rc::Rc;

fn arg<'a>(a: &'a [Value], i: usize, f: &str) -> R<&'a Value> {
    a.get(i).map_or_else(|| rt(format!("{}: brak argumentu #{}", f, i + 1)), Ok)
}
fn s_arg(a: &[Value], i: usize, f: &str) -> R<Rc<str>> {
    match arg(a, i, f)? { Value::Str(s) => Ok(s.clone()), o => rt(format!("{}: argument #{} powinien być tekstem, jest {}", f, i + 1, o.type_name())) }
}
fn i_arg(a: &[Value], i: usize, f: &str) -> R<i64> {
    match arg(a, i, f)? { Value::Int(n) => Ok(*n), o => rt(format!("{}: argument #{} powinien być liczbą całkowitą, jest {}", f, i + 1, o.type_name())) }
}
fn f_arg(a: &[Value], i: usize, f: &str) -> R<f64> {
    match arg(a, i, f)? { Value::Int(n) => Ok(*n as f64), Value::Float(x) => Ok(*x), o => rt(format!("{}: argument #{} powinien być liczbą, jest {}", f, i + 1, o.type_name())) }
}
fn a_arg(a: &[Value], i: usize, f: &str) -> R<Rc<ZArray>> {
    match arg(a, i, f)? { Value::Array(x) => Ok(x.clone()), o => rt(format!("{}: argument #{} powinien być tablicą, jest {}", f, i + 1, o.type_name())) }
}
fn bytes_to_string(b: &[u8]) -> String { String::from_utf8_lossy(b).into_owned() }
fn first_byte(s: &str) -> Option<u8> { s.as_bytes().first().copied() }

impl Interp {
    pub fn builtin_const(&self, m: &str, name: &str) -> Option<Value> {
        match (m, name) {
            ("Math", "PI") => Some(Value::Float(std::f64::consts::PI)),
            ("Math", "E") => Some(Value::Float(std::f64::consts::E)),
            ("System", "version") => Some(Value::str(env!("CARGO_PKG_VERSION"))),
            _ => None,
        }
    }

    pub fn call_builtin(&mut self, name: &str, args: &CallArgs) -> Option<R<Value>> {
        let a: Vec<Value> = args.iter().map(|(_, v)| v.clone()).collect();
        let a = &a[..];
        Some(match name {
            "println" => { let s: Vec<String> = a.iter().map(to_text).collect(); let _ = writeln!(self.out, "{}", s.join(" ")); Ok(Value::Null) }
            "print" => { for v in a { let _ = write!(self.out, "{}", to_text(v)); } Ok(Value::Null) }
            "eprintln" => { self.flush(); let s: Vec<String> = a.iter().map(to_text).collect(); eprintln!("{}", s.join(" ")); Ok(Value::Null) }
            "dump" => { for v in a { let _ = writeln!(self.out, "{}", dump(v)); } Ok(Value::Null) }
            "exit" => { let c = match a.first() { Some(Value::Int(c)) => *c as i32, _ => 0 }; Err(Flow::Exit(c)) }
            "assert" => { if arg(a, 0, name).map(|v| v.truthy()).unwrap_or(false) { Ok(Value::Null) } else {
                rt(match a.get(1) { Some(m) => format!("assert nie powiodło się: {}", to_text(m)), None => "assert nie powiodło się".into() }) } }
            "read_line" => { self.flush(); let mut l = String::new(); match std::io::stdin().read_line(&mut l) { Ok(0) | Err(_) => Ok(Value::Null), Ok(_) => Ok(Value::string(l.trim_end_matches(['\n', '\r']).to_string())) } }
            "typeof" => arg(a, 0, name).map(|v| Value::str(match v { Value::Object(o) => return_class(o), v => v.type_name().to_string() }.as_str())),
            "is_int" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Int(_)))),
            "is_float" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Float(_)))),
            "is_string" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Str(_)))),
            "is_bool" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Bool(_)))),
            "is_array" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Array(_)))),
            "is_null" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Null))),
            "is_object" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Object(_)))),
            "is_callable" => arg(a, 0, name).map(|v| Value::Bool(matches!(v, Value::Func(_)))),
            "intval" => match arg(a, 0, name) {
                Ok(Value::Int(i)) => Ok(Value::Int(*i)), Ok(Value::Float(f)) => Ok(Value::Int(*f as i64)),
                Ok(Value::Bool(b)) => Ok(Value::Int(*b as i64)), Ok(Value::Null) => Ok(Value::Int(0)),
                Ok(Value::Str(s)) => Ok(Value::Int(s.trim().parse::<i64>().or_else(|_| s.trim().parse::<f64>().map(|f| f as i64)).unwrap_or(0))),
                Ok(o) => rt(format!("intval: nie można przekonwertować {}", o.type_name())), Err(e) => Err(e) },
            "floatval" => match arg(a, 0, name) {
                Ok(Value::Int(i)) => Ok(Value::Float(*i as f64)), Ok(Value::Float(f)) => Ok(Value::Float(*f)),
                Ok(Value::Str(s)) => Ok(Value::Float(s.trim().parse().unwrap_or(0.0))),
                Ok(_) => Ok(Value::Float(0.0)), Err(e) => Err(e) },
            "strval" => arg(a, 0, name).map(|v| Value::string(to_text(v))),
            "boolval" => arg(a, 0, name).map(|v| Value::Bool(v.truthy())),
            "strlen" => s_arg(a, 0, name).map(|s| Value::Int(s.len() as i64)),
            "count" => match arg(a, 0, name) { Ok(Value::Array(x)) => Ok(Value::Int(x.len() as i64)), Ok(Value::Str(s)) => Ok(Value::Int(s.len() as i64)), Ok(o) => rt(format!("count: niedostępne dla {}", o.type_name())), Err(e) => Err(e) },
            "substr" => (|| {
                let s = s_arg(a, 0, name)?; let len = s.len() as i64;
                let mut st = i_arg(a, 1, name)?; if st < 0 { st = (len + st).max(0); } let st = st.min(len);
                let l = match a.get(2) { Some(Value::Int(l)) if *l >= 0 => *l, Some(Value::Int(l)) => (len - st + l).max(0), _ => len - st };
                let end = (st + l).min(len);
                Ok(Value::string(bytes_to_string(&s.as_bytes()[st as usize..end as usize])))
            })(),
            "ord" => s_arg(a, 0, name).and_then(|s| first_byte(&s).map(|b| Value::Int(b as i64)).ok_or_else(|| Flow::Err("ord: pusty tekst".into()))),
            "chr" => i_arg(a, 0, name).map(|n| Value::string(if n < 128 && n >= 0 { (n as u8 as char).to_string() } else { char::from_u32(n as u32).map(|c| c.to_string()).unwrap_or_default() })),
            "trim" => s_arg(a, 0, name).map(|s| Value::str(s.trim())),
            "upper" => s_arg(a, 0, name).map(|s| Value::string(s.to_uppercase())),
            "lower" => s_arg(a, 0, name).map(|s| Value::string(s.to_lowercase())),
            "replace" => (|| Ok(Value::string(s_arg(a, 0, name)?.replace(&*s_arg(a, 1, name)?, &s_arg(a, 2, name)?)))) (),
            "contains" => (|| Ok(Value::Bool(s_arg(a, 0, name)?.contains(&*s_arg(a, 1, name)?))))(),
            "starts_with" => (|| Ok(Value::Bool(s_arg(a, 0, name)?.starts_with(&*s_arg(a, 1, name)?))))(),
            "ends_with" => (|| Ok(Value::Bool(s_arg(a, 0, name)?.ends_with(&*s_arg(a, 1, name)?))))(),
            "index_of" => (|| Ok(Value::Int(s_arg(a, 0, name)?.find(&*s_arg(a, 1, name)?).map_or(-1, |i| i as i64))))(),
            "repeat" => (|| Ok(Value::string(s_arg(a, 0, name)?.repeat(i_arg(a, 1, name)?.max(0) as usize))))(),
            "split" => (|| { let s = s_arg(a, 0, name)?; let sep = s_arg(a, 1, name)?;
                Ok(Value::array(if sep.is_empty() { s.chars().map(|c| Value::string(c.to_string())).collect() } else { s.split(&*sep).map(Value::str).collect() })) })(),
            "chars" => s_arg(a, 0, name).map(|s| Value::array(s.chars().map(|c| Value::string(c.to_string())).collect())),
            "join" => (|| { let arr = a_arg(a, 0, name)?; let sep = s_arg(a, 1, name).unwrap_or_else(|_| Rc::from(""));
                Ok(Value::string(arr.entries.iter().map(|(_, v)| to_text(v)).collect::<Vec<_>>().join(&sep))) })(),
            "pad_left" | "pad_right" => (|| { let s = s_arg(a, 0, name)?; let n = i_arg(a, 1, name)?.max(0) as usize;
                let p = s_arg(a, 2, name).unwrap_or_else(|_| Rc::from(" ")); let p = p.chars().next().unwrap_or(' ');
                let l = s.chars().count(); if l >= n { return Ok(Value::Str(s)); }
                let pad: String = std::iter::repeat(p).take(n - l).collect();
                Ok(Value::string(if name == "pad_left" { format!("{}{}", pad, s) } else { format!("{}{}", s, pad) })) })(),
            "to_hex" => i_arg(a, 0, name).map(|n| Value::string(format!("{:x}", n))),
            "is_digit" => s_arg(a, 0, name).map(|s| Value::Bool(first_byte(&s).map_or(false, |b| b.is_ascii_digit()))),
            "is_alpha" => s_arg(a, 0, name).map(|s| Value::Bool(first_byte(&s).map_or(false, |b| b.is_ascii_alphabetic() || b == b'_' || b >= 0x80))),
            "is_alnum" => s_arg(a, 0, name).map(|s| Value::Bool(first_byte(&s).map_or(false, |b| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80))),
            "is_space" => s_arg(a, 0, name).map(|s| Value::Bool(first_byte(&s).map_or(false, |b| b.is_ascii_whitespace()))),
            "is_hex_digit" => s_arg(a, 0, name).map(|s| Value::Bool(first_byte(&s).map_or(false, |b| b.is_ascii_hexdigit()))),
            "keys" => a_arg(a, 0, name).map(|x| Value::array(x.entries.iter().map(|(k, _)| key_to_value(k)).collect())),
            "values" => a_arg(a, 0, name).map(|x| Value::array(x.entries.iter().map(|(_, v)| v.clone()).collect())),
            "has_key" => (|| { let x = a_arg(a, 0, name)?; let k = to_key(arg(a, 1, name)?).map_err(Flow::Err)?; Ok(Value::Bool(x.get(&k).is_some())) })(),
            "in_array" => (|| { let x = a_arg(a, 1, name)?; let n = arg(a, 0, name)?; Ok(Value::Bool(x.entries.iter().any(|(_, v)| strict_eq(v, n)))) })(),
            "push" => (|| { let mut x = a_arg(a, 0, name)?; let arr = Rc::make_mut(&mut x); for v in &a[1..] { arr.push(v.clone()); } Ok(Value::Array(x)) })(),
            "array_slice" => (|| { let x = a_arg(a, 0, name)?; let len = x.len() as i64;
                let mut st = i_arg(a, 1, name)?; if st < 0 { st = (len + st).max(0); } let st = st.min(len);
                let l = match a.get(2) { Some(Value::Int(l)) => *l, _ => len }; let end = (st + l.max(0)).min(len);
                Ok(Value::array(x.entries[st as usize..end as usize].iter().map(|(_, v)| v.clone()).collect())) })(),
            "reverse" => match arg(a, 0, name) {
                Ok(Value::Array(x)) => Ok(Value::array(x.entries.iter().rev().map(|(_, v)| v.clone()).collect())),
                Ok(Value::Str(s)) => Ok(Value::string(s.chars().rev().collect())),
                Ok(o) => rt(format!("reverse: niedostępne dla {}", o.type_name())), Err(e) => Err(e) },
            "range" => (|| { let (x, y) = (i_arg(a, 0, name)?, i_arg(a, 1, name)?);
                Ok(Value::array(if x <= y { (x..=y).map(Value::Int).collect() } else { (y..=x).rev().map(Value::Int).collect() })) })(),
            "sort" => (|| { let x = a_arg(a, 0, name)?; let mut v: Vec<Value> = x.entries.iter().map(|(_, v)| v.clone()).collect();
                let mut err = None;
                v.sort_by(|p, q| compare(p, q).unwrap_or_else(|e| { err = Some(e); std::cmp::Ordering::Equal }));
                if let Some(e) = err { return rt(format!("sort: {}", e)); }
                Ok(Value::array(v)) })(),
            "map" => (|| { let x = a_arg(a, 0, name)?; let f = arg(a, 1, name)?.clone(); let mut out = ZArray::new();
                for (k, v) in x.entries.iter() { let r = self.call_value(&f, vec![(None, v.clone())])?; out.set(k.clone(), r); }
                Ok(Value::Array(Rc::new(out))) })(),
            "filter" => (|| { let x = a_arg(a, 0, name)?; let f = arg(a, 1, name)?.clone(); let mut out = ZArray::new();
                for (k, v) in x.entries.iter() { if self.call_value(&f, vec![(None, v.clone())])?.truthy() { out.set(k.clone(), v.clone()); } }
                Ok(Value::Array(Rc::new(out))) })(),
            _ => return None,
        })
    }

    pub fn call_builtin_module(&mut self, m: &str, name: &str, args: &CallArgs) -> Option<R<Value>> {
        let a: Vec<Value> = args.iter().map(|(_, v)| v.clone()).collect();
        let a = &a[..];
        let f = format!("{}.{}", m, name);
        let f = f.as_str();
        let ioerr = |e: std::io::Error, p: &str| Flow::Err(format!("{}: {}", p, e));
        Some(match (m, name) {
            ("System", "os") => Ok(Value::str(std::env::consts::OS)),
            ("System", "architecture") => Ok(Value::str(std::env::consts::ARCH)),
            ("System", "cpuCount") => Ok(Value::Int(std::thread::available_parallelism().map_or(1, |n| n.get() as i64))),
            ("System", "args") => Ok(Value::array(self.args.iter().map(|s| Value::str(s)).collect())),
            ("System", "env") => s_arg(a, 0, f).map(|k| std::env::var(&*k).map(Value::string).unwrap_or(Value::Null)),
            ("System", "time") => Ok(Value::Float(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()))),
            ("System", "version") => Ok(Value::str(env!("CARGO_PKG_VERSION"))),
            ("System", "dirname") => s_arg(a, 0, f).map(|p| Value::string(std::path::Path::new(&*p).parent().map(|d| d.to_string_lossy().into_owned()).filter(|d| !d.is_empty()).unwrap_or_else(|| ".".into()))),
            // rozwiązuje ścieżkę importu (tablica nazw) względem katalogu — ta sama logika co `import` (Z#.toml, ZS_PATH)
            ("System", "resolveImport") => (|| {
                let segs = a_arg(a, 0, f)?; let dir = s_arg(a, 1, f)?;
                let names: Vec<String> = segs.entries.iter().map(|(_, v)| to_text(v)).collect();
                let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                Ok(resolve_import_path(std::path::Path::new(&*dir), &refs).map(|p| Value::string(p.to_string_lossy().into_owned())).unwrap_or(Value::Null))
            })(),
            ("System", "exit") => Err(Flow::Exit(match a.first() { Some(Value::Int(c)) => *c as i32, _ => 0 })),
            ("Math", "sqrt") => f_arg(a, 0, f).map(|x| Value::Float(x.sqrt())),
            ("Math", "floor") => f_arg(a, 0, f).map(|x| Value::Int(x.floor() as i64)),
            ("Math", "ceil") => f_arg(a, 0, f).map(|x| Value::Int(x.ceil() as i64)),
            ("Math", "round") => f_arg(a, 0, f).map(|x| Value::Int(x.round() as i64)),
            ("Math", "abs") => match arg(a, 0, f) { Ok(Value::Int(i)) => Ok(Value::Int(i.abs())), Ok(Value::Float(x)) => Ok(Value::Float(x.abs())), Ok(o) => rt(format!("Math.abs: {}", o.type_name())), Err(e) => Err(e) },
            ("Math", "pow") => (|| binop(zs_parser::ast::BinOp::Pow, arg(a, 0, f)?, arg(a, 1, f)?).map_err(Flow::Err))(),
            ("Math", "min") | ("Math", "max") => (|| { let (x, y) = (arg(a, 0, f)?, arg(a, 1, f)?);
                let o = compare(x, y).map_err(Flow::Err)?; Ok(if (name == "min") == o.is_le() { x.clone() } else { y.clone() }) })(),
            ("File", "read") => s_arg(a, 0, f).and_then(|p| std::fs::read_to_string(&*p).map(Value::string).map_err(|e| ioerr(e, &p))),
            ("File", "write") => (|| { let p = s_arg(a, 0, f)?; std::fs::write(&*p, to_text(arg(a, 1, f)?)).map_err(|e| ioerr(e, &p))?; Ok(Value::Null) })(),
            ("File", "append") => (|| { let p = s_arg(a, 0, f)?;
                let mut fh = std::fs::OpenOptions::new().create(true).append(true).open(&*p).map_err(|e| ioerr(e, &p))?;
                fh.write_all(to_text(arg(a, 1, f)?).as_bytes()).map_err(|e| ioerr(e, &p))?; Ok(Value::Null) })(),
            ("File", "exists") => s_arg(a, 0, f).map(|p| Value::Bool(std::path::Path::new(&*p).is_file())),
            ("File", "delete") => s_arg(a, 0, f).and_then(|p| std::fs::remove_file(&*p).map(|_| Value::Null).map_err(|e| ioerr(e, &p))),
            ("Directory", "create") => s_arg(a, 0, f).and_then(|p| std::fs::create_dir_all(&*p).map(|_| Value::Null).map_err(|e| ioerr(e, &p))),
            ("Directory", "exists") => s_arg(a, 0, f).map(|p| Value::Bool(std::path::Path::new(&*p).is_dir())),
            ("Directory", "list") => s_arg(a, 0, f).and_then(|p| {
                let mut v: Vec<String> = std::fs::read_dir(&*p).map_err(|e| ioerr(e, &p))?
                    .filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                v.sort(); Ok(Value::array(v.into_iter().map(Value::string).collect())) }),
            // Process.run(cmd, [args]) -> ["status" => int, "stdout" => string, "stderr" => string]
            ("Process", "run") => (|| { self.flush();
                let cmd = s_arg(a, 0, f)?;
                let argv: Vec<String> = match a.get(1) { Some(Value::Array(x)) => x.entries.iter().map(|(_, v)| to_text(v)).collect(), _ => vec![] };
                let out = std::process::Command::new(&*cmd).args(&argv).output().map_err(|e| ioerr(e, &cmd))?;
                let mut r = ZArray::new();
                r.set(Key::Str(Rc::from("status")), Value::Int(out.status.code().unwrap_or(-1) as i64));
                r.set(Key::Str(Rc::from("stdout")), Value::string(bytes_to_string(&out.stdout)));
                r.set(Key::Str(Rc::from("stderr")), Value::string(bytes_to_string(&out.stderr)));
                Ok(Value::Array(Rc::new(r))) })(),
            // Process.exec(cmd, [args]) -> status; dziedziczy stdin/stdout/stderr
            ("Process", "exec") => (|| { self.flush();
                let cmd = s_arg(a, 0, f)?;
                let argv: Vec<String> = match a.get(1) { Some(Value::Array(x)) => x.entries.iter().map(|(_, v)| to_text(v)).collect(), _ => vec![] };
                let st = std::process::Command::new(&*cmd).args(&argv).status().map_err(|e| ioerr(e, &cmd))?;
                Ok(Value::Int(st.code().unwrap_or(-1) as i64)) })(),
            // stage0: zadania wykonują się synchronicznie
            ("Task", "run") => (|| { let fv = arg(a, 0, f)?.clone(); self.call_value(&fv, vec![]) })(),
            _ => return None,
        })
    }
}

fn return_class(o: &Rc<std::cell::RefCell<Object>>) -> String { o.borrow().class.to_string() }
