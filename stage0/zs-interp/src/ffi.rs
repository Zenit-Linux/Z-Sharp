use crate::interp::*;
use crate::value::*;
use std::ffi::{c_char, c_void, CStr, CString};
use zs_parser::ast::ExternItem;

#[link(name = "dl")]
extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();
const MAX_ARGS: usize = 6;

fn is_string(types: &[std::rc::Rc<str>]) -> bool { types.iter().any(|t| t.eq_ignore_ascii_case("string")) }

pub fn call_extern(item: &ExternItem, args: &CallArgs) -> R<Value> {
    let name = &*item.name;
    if args.len() != item.params.len() {
        return rt(format!("funkcja extern `{}`: oczekiwano {} argumentów, podano {}", name, item.params.len(), args.len()));
    }
    if args.len() > MAX_ARGS {
        return rt(format!("funkcja extern `{}`: obsługiwane jest co najwyżej {} argumentów", name, MAX_ARGS));
    }
    let cname = CString::new(name).map_err(|_| Flow::Err(format!("zła nazwa extern `{}`", name)))?;
    // SAFETY: dlsym z RTLD_DEFAULT i poprawnym napisem C.
    let sym = unsafe { dlsym(RTLD_DEFAULT, cname.as_ptr()) };
    if sym.is_null() { return rt(format!("funkcja extern `{}`: symbol nie został znaleziony w libc", name)); }

    let mut keep: Vec<CString> = Vec::new();
    let mut regs = [0i64; MAX_ARGS];
    for (i, ((_, v), p)) in args.iter().zip(item.params.iter()).enumerate() {
        regs[i] = if is_string(&p.types) {
            match v {
                Value::Null => 0,
                Value::Str(s) => {
                    let c = CString::new(s.as_bytes()).map_err(|_| Flow::Err(format!("extern `{}`: napis zawiera NUL (argument {})", name, i + 1)))?;
                    let ptr = c.as_ptr() as i64;
                    keep.push(c);
                    ptr
                }
                o => return rt(format!("extern `{}`: argument {} ma być napisem, jest {}", name, i + 1, o.type_name())),
            }
        } else {
            match v {
                Value::Int(n) => *n,
                Value::Bool(b) => *b as i64,
                Value::Null => 0,
                Value::Float(f) => *f as i64,
                o => return rt(format!("extern `{}`: argument {} ma być liczbą, jest {}", name, i + 1, o.type_name())),
            }
        };
    }
    let r = unsafe { invoke(sym, args.len(), &regs) };
    drop(keep);
    if is_string(&item.ret) {
        if r == 0 { return Ok(Value::Null); }
        // SAFETY: wynik funkcji deklarowanej jako zwracająca `char *` (NUL-terminated).
        let s = unsafe { CStr::from_ptr(r as *const c_char) };
        return Ok(Value::string(s.to_string_lossy().into_owned()));
    }
    Ok(Value::Int(r))
}

unsafe fn invoke(f: *mut c_void, n: usize, a: &[i64; MAX_ARGS]) -> i64 {
    type F0 = unsafe extern "C" fn() -> i64;
    type F1 = unsafe extern "C" fn(i64) -> i64;
    type F2 = unsafe extern "C" fn(i64, i64) -> i64;
    type F3 = unsafe extern "C" fn(i64, i64, i64) -> i64;
    type F4 = unsafe extern "C" fn(i64, i64, i64, i64) -> i64;
    type F5 = unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64;
    type F6 = unsafe extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64;
    match n {
        0 => std::mem::transmute::<*mut c_void, F0>(f)(),
        1 => std::mem::transmute::<*mut c_void, F1>(f)(a[0]),
        2 => std::mem::transmute::<*mut c_void, F2>(f)(a[0], a[1]),
        3 => std::mem::transmute::<*mut c_void, F3>(f)(a[0], a[1], a[2]),
        4 => std::mem::transmute::<*mut c_void, F4>(f)(a[0], a[1], a[2], a[3]),
        5 => std::mem::transmute::<*mut c_void, F5>(f)(a[0], a[1], a[2], a[3], a[4]),
        _ => std::mem::transmute::<*mut c_void, F6>(f)(a[0], a[1], a[2], a[3], a[4], a[5]),
    }
}

#[repr(C)]
struct SigSet([u64; 16]);
extern "C" {
    fn sigfillset(set: *mut SigSet) -> i32;
    fn pthread_sigmask(how: i32, set: *const SigSet, old: *mut SigSet) -> i32;
}

/// Wątek główny tylko czeka na wątek interpretera, więc blokuje wszystkie sygnały: sygnały skierowane do procesu
/// (np. SIGCHLD) trafiają wtedy do wątku interpretera albo — gdy ten je blokuje (`sigprocmask` + `signalfd`
/// w zenitd) — pozostają oczekujące w procesie i są widoczne przez `signalfd`.
pub fn shield_main_thread_signals() {
    const SIG_BLOCK: i32 = 0;
    let mut set = SigSet([0; 16]);
    // SAFETY: `set` ma rozmiar >= sigset_t (glibc: 128 B, musl: 128 B).
    unsafe { sigfillset(&mut set); pthread_sigmask(SIG_BLOCK, &set, std::ptr::null_mut()); }
}
