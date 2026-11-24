use crate::interp::*;
use crate::value::*;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::rc::Rc;
use std::time::Duration;

pub enum Sock {
    Listener(TcpListener),
    Stream { s: TcpStream, timeout: Option<Duration> },
    Udp { s: UdpSocket },
}

#[repr(C)]
struct Tm { sec: i32, min: i32, hour: i32, mday: i32, mon: i32, year: i32, wday: i32, yday: i32, isdst: i32, gmtoff: i64, zone: *const i8 }
extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    fn gethostname(name: *mut u8, len: usize) -> i32;
}

fn local_offset(ts: i64) -> i64 {
    let mut tm = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, gmtoff: 0, zone: std::ptr::null() };
    // SAFETY: `tm` jest poprawnie zainicjowaną strukturą o układzie zgodnym z `struct tm` (glibc/musl/BSD).
    let r = unsafe { localtime_r(&ts, &mut tm) };
    if r.is_null() { 0 } else { tm.gmtoff }
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: bufor ma 256 bajtów, przekazujemy jego długość.
    let ok = unsafe { gethostname(buf.as_mut_ptr(), buf.len() - 1) } == 0;
    if !ok { return "localhost".into(); }
    let n = buf.iter().position(|&b| b == 0).unwrap_or(0);
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn key(s: &str) -> Key { Key::Str(Rc::from(s)) }
fn assoc(items: Vec<(&str, Value)>) -> Value {
    let mut a = ZArray::new();
    for (k, v) in items { a.set(key(k), v); }
    Value::Array(Rc::new(a))
}
fn arg<'a>(a: &'a [Value], i: usize, f: &str) -> R<&'a Value> {
    a.get(i).map_or_else(|| rt(format!("{f}: brak argumentu #{}", i + 1)), Ok)
}
fn s_arg(a: &[Value], i: usize, f: &str) -> R<Rc<str>> {
    match arg(a, i, f)? { Value::Str(s) => Ok(s.clone()), o => rt(format!("{f}: argument #{} powinien być tekstem, jest {}", i + 1, o.type_name())) }
}
fn i_arg(a: &[Value], i: usize, f: &str) -> R<i64> {
    match arg(a, i, f)? { Value::Int(n) => Ok(*n), o => rt(format!("{f}: argument #{} powinien być liczbą całkowitą, jest {}", i + 1, o.type_name())) }
}
fn io<T>(r: std::io::Result<T>, what: &str) -> R<T> { r.map_err(|e| Flow::Err(format!("{what}: {e}"))) }

/// Skraca bufor do ostatniej pełnej sekwencji UTF-8; zwraca liczbę brakujących bajtów niepełnego znaku na końcu.
fn missing_tail(b: &[u8]) -> usize {
    let n = b.len();
    for back in 1..=3.min(n) {
        let c = b[n - back];
        if c & 0xC0 == 0x80 { continue; }
        let need: usize = if c >= 0xF0 { 4 } else if c >= 0xE0 { 3 } else if c >= 0xC0 { 2 } else { 1 };
        return need.saturating_sub(back);
    }
    0
}

impl Interp {
    fn sock_new(&mut self, s: Sock) -> Value {
        for (i, slot) in self.socks.iter_mut().enumerate() {
            if slot.is_none() { *slot = Some(s); return Value::Int(i as i64 + 1); }
        }
        self.socks.push(Some(s));
        Value::Int(self.socks.len() as i64)
    }
    fn sock(&mut self, v: &Value, f: &str) -> R<&mut Sock> {
        let Value::Int(h) = v else { return rt(format!("{f}: uchwyt gniazda powinien być liczbą całkowitą")) };
        let idx = (*h as usize).wrapping_sub(1);
        match self.socks.get_mut(idx) { Some(Some(s)) => Ok(s), _ => rt(format!("{f}: nieprawidłowy uchwyt gniazda {h}")) }
    }

    pub fn call_sys(&mut self, m: &str, name: &str, a: &[Value]) -> Option<R<Value>> {
        let f = format!("{m}.{name}");
        let f = f.as_str();
        Some(match (m, name) {
            ("System", "setEnv") => (|| { let (k, v) = (s_arg(a, 0, f)?, s_arg(a, 1, f)?);
                if k.is_empty() || k.contains(['=', '\0']) || v.contains('\0') { return rt(format!("{f}: niepoprawna nazwa lub wartość")); }
                std::env::set_var(&*k, &*v); Ok(Value::Null) })(),
            ("System", "unsetEnv") => s_arg(a, 0, f).map(|k| { if !k.is_empty() && !k.contains(['=', '\0']) { std::env::remove_var(&*k); } Value::Null }),
            ("System", "environ") => {
                let mut kv: Vec<(String, String)> = std::env::vars_os().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())).collect();
                kv.sort();
                let mut arr = ZArray::new();
                for (k, v) in kv { arr.set(Key::Str(Rc::from(k)), Value::string(v)); }
                Ok(Value::Array(Rc::new(arr)))
            }
            ("System", "cwd") => io(std::env::current_dir(), f).map(|p| Value::string(p.to_string_lossy().into_owned())),
            ("System", "chdir") => s_arg(a, 0, f).and_then(|p| io(std::env::set_current_dir(&*p), &p).map(|_| Value::Null)),
            ("System", "pid") => Ok(Value::Int(std::process::id() as i64)),
            ("System", "hostname") => Ok(Value::string(hostname())),
            ("System", "nowNanos") => Ok(Value::Int(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64))),
            ("System", "monotonic") => Ok(Value::Int(self.started.elapsed().as_nanos() as i64 + 1)),
            ("System", "sleep") => i_arg(a, 0, f).map(|ms| { self.flush(); std::thread::sleep(Duration::from_millis(ms.max(0) as u64)); Value::Null }),
            ("System", "localOffset") => i_arg(a, 0, f).map(|t| Value::Int(local_offset(t))),
            // stage0 zarządza pamięcią przez Rc (zwalnianie natychmiastowe): gc() nic nie robi, statystyki są zerowe (te same klucze co w runtime natywnym)
            ("System", "gc") => Ok(Value::Null),
            ("System", "gcStats") => Ok(assoc(["collections", "allocated", "freed", "live", "heap", "peak"].iter().map(|k| (*k, Value::Int(0))).collect())),
            ("System", "bytes") => s_arg(a, 0, f).map(|s| Value::array(s.as_bytes().iter().map(|&b| Value::Int(b as i64)).collect())),
            ("System", "fromBytes") => (|| {
                let Value::Array(x) = arg(a, 0, f)? else { return rt(format!("{f}: argument #1 powinien być tablicą")) };
                let mut b = Vec::with_capacity(x.len());
                for (_, v) in x.entries.iter() {
                    match v { Value::Int(n) if (0..=255).contains(n) => b.push(*n as u8), _ => return rt(format!("{f}: element spoza zakresu 0..255")) }
                }
                Ok(Value::string(String::from_utf8_lossy(&b).into_owned()))
            })(),

            ("File", "stat") => s_arg(a, 0, f).map(|p| match std::fs::symlink_metadata(&*p) {
                Err(_) => Value::Null,
                Ok(md) => {
                    let t = md.file_type();
                    let kind = if t.is_symlink() { "link" } else if t.is_dir() { "dir" } else if t.is_file() { "file" } else { "other" };
                    assoc(vec![("kind", Value::str(kind)), ("size", Value::Int(md.len() as i64)),
                               ("mode", Value::Int((md.permissions().mode() & 0o7777) as i64)), ("mtime", Value::Int(md.mtime()))])
                }
            }),
            ("File", "rename") => (|| { let (x, y) = (s_arg(a, 0, f)?, s_arg(a, 1, f)?); io(std::fs::rename(&*x, &*y), &x)?; Ok(Value::Null) })(),
            ("File", "realpath") => s_arg(a, 0, f).and_then(|p| io(std::fs::canonicalize(&*p), &p).map(|r| Value::string(r.to_string_lossy().into_owned()))),
            ("File", "chmod") => (|| { let p = s_arg(a, 0, f)?; let m = i_arg(a, 1, f)?;
                io(std::fs::set_permissions(&*p, std::fs::Permissions::from_mode((m & 0o7777) as u32)), &p)?; Ok(Value::Null) })(),
            ("File", "symlink") => (|| { let (t, l) = (s_arg(a, 0, f)?, s_arg(a, 1, f)?); io(symlink(&*t, &*l), &l)?; Ok(Value::Null) })(),
            ("File", "readlink") => s_arg(a, 0, f).and_then(|p| io(std::fs::read_link(&*p), &p).map(|r| Value::string(r.to_string_lossy().into_owned()))),
            ("Directory", "remove") => s_arg(a, 0, f).and_then(|p| io(std::fs::remove_dir(&*p), &p).map(|_| Value::Null)),

            ("Socket", _) => return Some(self.call_socket(name, a, f)),
            _ => return None,
        })
    }

    fn call_socket(&mut self, name: &str, a: &[Value], f: &str) -> R<Value> {
        let addr_of = |host: &str, port: i64| -> R<SocketAddr> {
            io((host, port as u16).to_socket_addrs(), &format!("{f}({host}:{port})"))?.next()
                .map_or_else(|| rt(format!("{f}: nie można rozwiązać {host}")), Ok)
        };
        let addr_val = |sa: SocketAddr| assoc(vec![("host", Value::string(sa.ip().to_string())), ("port", Value::Int(sa.port() as i64))]);
        match name {
            "tcpConnect" => {
                let (host, port, ms) = (s_arg(a, 0, f)?, i_arg(a, 1, f)?, i_arg(a, 2, f)?);
                let sa = addr_of(&host, port)?;
                let s = io(if ms > 0 { TcpStream::connect_timeout(&sa, Duration::from_millis(ms as u64)) } else { TcpStream::connect(sa) }, &format!("{f}({host}:{port})"))?;
                let _ = s.set_nodelay(true);
                Ok(self.sock_new(Sock::Stream { s, timeout: None }))
            }
            "tcpListen" => {
                let (host, port) = (s_arg(a, 0, f)?, i_arg(a, 1, f)?);
                let sa = addr_of(&host, port)?;
                let l = io(TcpListener::bind(sa), &format!("{f}({host}:{port})"))?;
                Ok(self.sock_new(Sock::Listener(l)))
            }
            "accept" => {
                self.flush();
                let Sock::Listener(l) = self.sock(arg(a, 0, f)?, f)? else { return rt(format!("{f}: to nie jest gniazdo nasłuchujące")) };
                let (s, peer) = io(l.accept(), f)?;
                let _ = s.set_nodelay(true);
                let h = self.sock_new(Sock::Stream { s, timeout: None });
                Ok(assoc(vec![("handle", h), ("host", Value::string(peer.ip().to_string())), ("port", Value::Int(peer.port() as i64))]))
            }
            "read" => {
                let max = i_arg(a, 1, f)?.clamp(1, 1 << 24) as usize;
                let Sock::Stream { s, timeout } = self.sock(arg(a, 0, f)?, f)? else { return rt(format!("{f}: to nie jest połączenie TCP")) };
                let mut buf = vec![0u8; max];
                let n = match s.read(&mut buf) {
                    Ok(n) => n,
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(Value::Null),
                    Err(e) => return rt(format!("{f}: {e}")),
                };
                buf.truncate(n);
                // dobierz brakujące bajty niepełnego znaku UTF-8 na końcu porcji
                let mut miss = missing_tail(&buf);
                let _ = timeout;
                while miss > 0 {
                    let mut one = vec![0u8; miss];
                    match s.read(&mut one) { Ok(0) | Err(_) => break, Ok(k) => { buf.extend_from_slice(&one[..k]); miss = missing_tail(&buf); } }
                }
                Ok(Value::string(String::from_utf8_lossy(&buf).into_owned()))
            }
            "write" => {
                let data = match arg(a, 1, f)? { Value::Str(s) => s.clone(), o => Rc::from(to_text(o)) };
                let Sock::Stream { s, .. } = self.sock(arg(a, 0, f)?, f)? else { return rt(format!("{f}: to nie jest połączenie TCP")) };
                io(s.write_all(data.as_bytes()), f)?;
                io(s.flush(), f)?;
                Ok(Value::Int(data.len() as i64))
            }
            "close" => {
                let Value::Int(h) = arg(a, 0, f)? else { return rt(format!("{f}: nieprawidłowy uchwyt")) };
                let idx = (*h as usize).wrapping_sub(1);
                if let Some(slot) = self.socks.get_mut(idx) {
                    if let Some(Sock::Stream { s, .. }) = slot { let _ = s.shutdown(Shutdown::Both); }
                    *slot = None;
                }
                Ok(Value::Null)
            }
            "setTimeout" => {
                let ms = i_arg(a, 1, f)?;
                let d = if ms > 0 { Some(Duration::from_millis(ms as u64)) } else { None };
                match self.sock(arg(a, 0, f)?, f)? {
                    Sock::Stream { s, timeout } => { let _ = s.set_read_timeout(d); let _ = s.set_write_timeout(d); *timeout = d; }
                    Sock::Udp { s } => { let _ = s.set_read_timeout(d); let _ = s.set_write_timeout(d); }
                    Sock::Listener(_) => {}
                }
                Ok(Value::Null)
            }
            "udpBind" => {
                let (host, port) = (s_arg(a, 0, f)?, i_arg(a, 1, f)?);
                let sa = addr_of(&host, port)?;
                let s = io(UdpSocket::bind(sa), &format!("{f}({host}:{port})"))?;
                Ok(self.sock_new(Sock::Udp { s }))
            }
            "sendTo" => {
                let (host, port) = (s_arg(a, 1, f)?, i_arg(a, 2, f)?);
                let data = match arg(a, 3, f)? { Value::Str(s) => s.clone(), o => Rc::from(to_text(o)) };
                let sa = addr_of(&host, port)?;
                let Sock::Udp { s } = self.sock(arg(a, 0, f)?, f)? else { return rt(format!("{f}: to nie jest gniazdo UDP")) };
                io(s.send_to(data.as_bytes(), sa), f).map(|n| Value::Int(n as i64))
            }
            "recvFrom" => {
                let max = i_arg(a, 1, f)?.clamp(1, 1 << 20) as usize;
                let Sock::Udp { s } = self.sock(arg(a, 0, f)?, f)? else { return rt(format!("{f}: to nie jest gniazdo UDP")) };
                let mut buf = vec![0u8; max];
                match s.recv_from(&mut buf) {
                    Ok((n, from)) => Ok(assoc(vec![("data", Value::string(String::from_utf8_lossy(&buf[..n]).into_owned())),
                                                    ("host", Value::string(from.ip().to_string())), ("port", Value::Int(from.port() as i64))])),
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(Value::Null),
                    Err(e) => rt(format!("{f}: {e}")),
                }
            }
            "resolve" => {
                let host = s_arg(a, 0, f)?;
                let mut out: Vec<Value> = vec![];
                for sa in io((&*host, 0u16).to_socket_addrs(), &format!("{f}({host})"))? {
                    let ip = Value::string(sa.ip().to_string());
                    if !out.iter().any(|v| strict_eq(v, &ip)) { out.push(ip); }
                }
                Ok(Value::array(out))
            }
            "localAddr" => match self.sock(arg(a, 0, f)?, f)? {
                Sock::Listener(l) => io(l.local_addr(), f).map(addr_val),
                Sock::Stream { s, .. } => io(s.local_addr(), f).map(addr_val),
                Sock::Udp { s } => io(s.local_addr(), f).map(addr_val),
            },
            "peerAddr" => match self.sock(arg(a, 0, f)?, f)? {
                Sock::Stream { s, .. } => io(s.peer_addr(), f).map(addr_val),
                _ => rt(format!("{f}: tylko dla połączeń TCP")),
            },
            _ => rt(format!("moduł Socket nie ma funkcji {name}")),
        }
    }
}
