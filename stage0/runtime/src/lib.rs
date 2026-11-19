use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use zamm::{Config, Error, Handle, Heap, Lifetime, TaskId, Zamm};

pub const ZS_NULL: u64 = 0;
pub const ZS_BOOL: u64 = 1;
pub const ZS_INT: u64 = 2;
pub const ZS_FLOAT: u64 = 3;
pub const ZS_STR: u64 = 4;
pub const ZS_ARRAY: u64 = 5;
pub const ZS_OBJ: u64 = 6;

pub const ZS_FRAME: i32 = 0;
pub const ZS_TEMP: i32 = 1;
pub const ZS_PERSISTENT: i32 = 2;
// wartości >= 16 oznaczają zadanie: 16 + id zadania (zs_task_new zwraca gotowy kod)
pub const ZS_TASK_BASE: i64 = 16;

pub const ZS_OK: i32 = 0;
pub const ZS_E_OOM: i32 = 1;
pub const ZS_E_SCOPE: i32 = 2;
pub const ZS_E_REFERENCED: i32 = 3;
pub const ZS_E_STALE: i32 = 4;
pub const ZS_E_TYPE: i32 = 5;
pub const ZS_E_BOUNDS: i32 = 6;
pub const ZS_E_INTERNAL: i32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZValue { pub tag: u64, pub bits: u64 }

impl ZValue {
    pub const NULL: ZValue = ZValue { tag: ZS_NULL, bits: 0 };
    pub fn int(i: i64) -> ZValue { ZValue { tag: ZS_INT, bits: i as u64 } }
    pub fn float(f: f64) -> ZValue { ZValue { tag: ZS_FLOAT, bits: f.to_bits() } }
    pub fn boolean(b: bool) -> ZValue { ZValue { tag: ZS_BOOL, bits: b as u64 } }
    pub fn as_int(&self) -> Option<i64> { (self.tag == ZS_INT).then_some(self.bits as i64) }
    pub fn as_float(&self) -> Option<f64> { (self.tag == ZS_FLOAT).then(|| f64::from_bits(self.bits)) }
}

#[repr(C)] struct StrHdr { len: u64 }                                  // bajty bezpośrednio za nagłówkiem
#[repr(C)] struct ArrHdr { len: u64, cap: u64, data: *mut ZValue }

/// Zasób zewnętrzny z deterministycznym zwolnieniem (pliki, sockety, blokady) — destruktor C wywoływany LIFO.
struct Resource { ctx: *mut c_void, close: Option<extern "C" fn(*mut c_void)> }
impl Drop for Resource { fn drop(&mut self) { if let Some(f) = self.close.take() { f(self.ctx) } } }

pub struct Runtime { heap: Heap, zamm: Zamm }

fn lt(rt: &Runtime, code: i64) -> Option<Lifetime> {
    Some(match code {
        0 => Lifetime::Frame, 1 => Lifetime::Temp, 2 => Lifetime::Persistent,
        c if c >= ZS_TASK_BASE => { let id = TaskId((c - ZS_TASK_BASE) as u64); rt.heap.task_arena(id)?; Lifetime::Task(id) }
        _ => return None,
    })
}
fn code(e: Error) -> i32 {
    match e { Error::OutOfMemory => ZS_E_OOM, Error::NoScope | Error::UnknownTask => ZS_E_SCOPE, Error::Referenced(_) => ZS_E_REFERENCED,
              Error::Stale => ZS_E_STALE, _ => ZS_E_INTERNAL }
}

impl Runtime {
    pub fn new(cfg: Config) -> Runtime { let zamm = Zamm::new(cfg); Runtime { heap: zamm.heap(), zamm } }
    pub fn heap(&mut self) -> &mut Heap { &mut self.heap }
    pub fn zamm(&self) -> &Zamm { &self.zamm }

    pub fn str_new(&mut self, l: Lifetime, s: &[u8]) -> Result<ZValue, Error> {
        let p = self.heap.alloc_in(l, std::mem::size_of::<StrHdr>() + s.len(), 8)?;
        unsafe { (p as *mut StrHdr).write(StrHdr { len: s.len() as u64 }); std::ptr::copy_nonoverlapping(s.as_ptr(), p.add(8), s.len()); }
        Ok(ZValue { tag: ZS_STR, bits: p as u64 })
    }
    pub fn str_bytes(v: &ZValue) -> Option<&[u8]> {
        if v.tag != ZS_STR || v.bits == 0 { return None; }
        unsafe { let h = v.bits as *const StrHdr; Some(std::slice::from_raw_parts((h as *const u8).add(8), (*h).len as usize)) }
    }
    pub fn str_concat(&mut self, l: Lifetime, a: &ZValue, b: &ZValue) -> Result<ZValue, Error> {
        let (x, y) = (Self::str_bytes(a).ok_or(Error::BadLayout)?, Self::str_bytes(b).ok_or(Error::BadLayout)?);
        let mut v = Vec::with_capacity(x.len() + y.len()); v.extend_from_slice(x); v.extend_from_slice(y);
        self.str_new(l, &v)
    }
    pub fn array_new(&mut self, l: Lifetime, cap: usize) -> Result<ZValue, Error> {
        let cap = cap.max(4);
        let h = self.heap.alloc_in(l, std::mem::size_of::<ArrHdr>(), 8)? as *mut ArrHdr;
        let d = self.heap.alloc_in(l, cap * 16, 8)? as *mut ZValue;
        unsafe { h.write(ArrHdr { len: 0, cap: cap as u64, data: d }) };
        Ok(ZValue { tag: ZS_ARRAY, bits: h as u64 })
    }
    /// Dodaje element; przy zapełnieniu dane przenoszone są do większego bloku w tym samym czasie życia
    /// (stary blok wraca do puli razem z zakresem — koszt liniowo ograniczony: suma geometryczna).
    pub fn array_push(&mut self, l: Lifetime, arr: &ZValue, v: ZValue) -> Result<(), Error> {
        if arr.tag != ZS_ARRAY { return Err(Error::BadLayout); }
        let h = arr.bits as *mut ArrHdr;
        unsafe {
            if (*h).len == (*h).cap {
                let ncap = (*h).cap * 2;
                let nd = self.heap.alloc_in(l, ncap as usize * 16, 8)? as *mut ZValue;
                std::ptr::copy_nonoverlapping((*h).data, nd, (*h).len as usize);
                (*h).data = nd; (*h).cap = ncap;
            }
            (*h).data.add((*h).len as usize).write(v); (*h).len += 1;
        }
        Ok(())
    }
    pub fn array_len(arr: &ZValue) -> Option<usize> { (arr.tag == ZS_ARRAY).then(|| unsafe { (*(arr.bits as *const ArrHdr)).len as usize }) }
    pub fn array_get(arr: &ZValue, i: usize) -> Option<ZValue> {
        if arr.tag != ZS_ARRAY { return None; }
        unsafe { let h = arr.bits as *const ArrHdr; if (i as u64) < (*h).len { Some(*(*h).data.add(i)) } else { None } }
    }
    pub fn array_set(arr: &ZValue, i: usize, v: ZValue) -> Result<(), Error> {
        if arr.tag != ZS_ARRAY { return Err(Error::BadLayout); }
        unsafe {
            let h = arr.bits as *mut ArrHdr;
            if (i as u64) >= (*h).len { return Err(Error::Stale); }
            (*h).data.add(i).write(v);
        }
        Ok(())
    }
    pub fn resource(&mut self, l: Lifetime, ctx: *mut c_void, close: Option<extern "C" fn(*mut c_void)>) -> Result<Handle, Error> {
        self.heap.alloc_value(l, Resource { ctx, close })
    }
}

// ───────────────────────── ABI C ─────────────────────────
macro_rules! guard { ($e:expr) => { catch_unwind(AssertUnwindSafe(|| $e)).unwrap_or(ZS_E_INTERNAL) } }

/// Tworzy runtime. `chunk_kib`/`large_kib` = 0 oznacza wartość domyślną.
#[no_mangle] pub extern "C" fn zs_runtime_new(chunk_kib: u32, large_kib: u32) -> *mut Runtime {
    catch_unwind(|| {
        let mut c = Config::default();
        if chunk_kib > 0 { c.set("chunk-kib", chunk_kib as i64); }
        if large_kib > 0 { c.set("large-object-kib", large_kib as i64); }
        Box::into_raw(Box::new(Runtime::new(c)))
    }).unwrap_or(std::ptr::null_mut())
}
#[no_mangle] pub unsafe extern "C" fn zs_runtime_free(rt: *mut Runtime) { if !rt.is_null() { let _ = catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(rt)))); } }

#[no_mangle] pub unsafe extern "C" fn zs_frame_push(rt: *mut Runtime) { (*rt).heap.push_frame() }
#[no_mangle] pub unsafe extern "C" fn zs_frame_pop(rt: *mut Runtime) -> i32 { guard!((*rt).heap.pop_frame().map_or_else(code, |_| ZS_OK)) }
#[no_mangle] pub unsafe extern "C" fn zs_temp_begin(rt: *mut Runtime) { (*rt).heap.begin_temp() }
#[no_mangle] pub unsafe extern "C" fn zs_temp_end(rt: *mut Runtime) -> i32 { guard!((*rt).heap.end_temp().map_or_else(code, |_| ZS_OK)) }
/// Zwraca kod czasu życia zadania (>= 16) do użycia w `zs_str_new` itd.
#[no_mangle] pub unsafe extern "C" fn zs_task_new(rt: *mut Runtime) -> i64 { ZS_TASK_BASE + (*rt).heap.new_task().0 as i64 }
#[no_mangle] pub unsafe extern "C" fn zs_task_end(rt: *mut Runtime, task: i64) -> i32 {
    guard!((*rt).heap.end_task(TaskId((task - ZS_TASK_BASE) as u64)).map_or_else(code, |_| ZS_OK))
}

#[no_mangle] pub unsafe extern "C" fn zs_str_new(rt: *mut Runtime, life: i64, p: *const u8, n: usize, out: *mut ZValue) -> i32 {
    guard!({
        let rt = &mut *rt;
        let Some(l) = lt(rt, life) else { return ZS_E_SCOPE };
        let s = if n == 0 { &[][..] } else { std::slice::from_raw_parts(p, n) };
        match rt.str_new(l, s) { Ok(v) => { *out = v; ZS_OK } Err(e) => code(e) }
    })
}
#[no_mangle] pub unsafe extern "C" fn zs_str_len(v: ZValue) -> i64 { Runtime::str_bytes(&v).map_or(-1, |b| b.len() as i64) }
#[no_mangle] pub unsafe extern "C" fn zs_str_ptr(v: ZValue) -> *const u8 { Runtime::str_bytes(&v).map_or(std::ptr::null(), |b| b.as_ptr()) }
#[no_mangle] pub unsafe extern "C" fn zs_str_concat(rt: *mut Runtime, life: i64, a: ZValue, b: ZValue, out: *mut ZValue) -> i32 {
    guard!({
        let rt = &mut *rt;
        let Some(l) = lt(rt, life) else { return ZS_E_SCOPE };
        match rt.str_concat(l, &a, &b) { Ok(v) => { *out = v; ZS_OK } Err(Error::BadLayout) => ZS_E_TYPE, Err(e) => code(e) }
    })
}
#[no_mangle] pub unsafe extern "C" fn zs_array_new(rt: *mut Runtime, life: i64, cap: usize, out: *mut ZValue) -> i32 {
    guard!({ let rt = &mut *rt; let Some(l) = lt(rt, life) else { return ZS_E_SCOPE };
        match rt.array_new(l, cap) { Ok(v) => { *out = v; ZS_OK } Err(e) => code(e) } })
}
#[no_mangle] pub unsafe extern "C" fn zs_array_push(rt: *mut Runtime, life: i64, arr: ZValue, v: ZValue) -> i32 {
    guard!({ let rt = &mut *rt; let Some(l) = lt(rt, life) else { return ZS_E_SCOPE };
        match rt.array_push(l, &arr, v) { Ok(()) => ZS_OK, Err(Error::BadLayout) => ZS_E_TYPE, Err(e) => code(e) } })
}
#[no_mangle] pub unsafe extern "C" fn zs_array_len(arr: ZValue) -> i64 { Runtime::array_len(&arr).map_or(-1, |n| n as i64) }
#[no_mangle] pub unsafe extern "C" fn zs_array_get(arr: ZValue, i: usize, out: *mut ZValue) -> i32 {
    match Runtime::array_get(&arr, i) { Some(v) => { *out = v; ZS_OK } None => if arr.tag == ZS_ARRAY { ZS_E_BOUNDS } else { ZS_E_TYPE } }
}
/// Nadpisuje element `i` (< len). ZS_E_TYPE dla nie-tablicy, ZS_E_BOUNDS poza zakresem.
#[no_mangle] pub unsafe extern "C" fn zs_array_set(arr: ZValue, i: usize, v: ZValue) -> i32 {
    match Runtime::array_set(&arr, i, v) { Ok(()) => ZS_OK, Err(Error::BadLayout) => ZS_E_TYPE, Err(_) => ZS_E_BOUNDS }
}
/// Rejestruje zasób; `close(ctx)` wykona się deterministycznie przy końcu zakresu (LIFO) lub `zs_release`.
#[no_mangle] pub unsafe extern "C" fn zs_resource_new(rt: *mut Runtime, life: i64, ctx: *mut c_void, close: Option<extern "C" fn(*mut c_void)>, out: *mut u64) -> i32 {
    guard!({ let rt = &mut *rt; let Some(l) = lt(rt, life) else { return ZS_E_SCOPE };
        match rt.resource(l, ctx, close) { Ok(h) => { *out = h.raw(); ZS_OK } Err(e) => code(e) } })
}
#[no_mangle] pub unsafe extern "C" fn zs_retain(rt: *mut Runtime, h: u64) -> i32 { if (*rt).heap.retain(Handle::from_raw(h)) { ZS_OK } else { ZS_E_STALE } }
/// Zwraca 1, gdy obiekt został zniszczony (ostatnie odwołanie do obiektu trwałego).
#[no_mangle] pub unsafe extern "C" fn zs_release(rt: *mut Runtime, h: u64) -> i32 { (*rt).heap.release(Handle::from_raw(h)) as i32 }
#[no_mangle] pub unsafe extern "C" fn zs_handle_live(rt: *mut Runtime, h: u64) -> i32 { (*rt).heap.is_live(Handle::from_raw(h)) as i32 }

#[repr(C)] #[derive(Default)]
pub struct ZMemStats {
    pub frame_used: u64, pub temp_used: u64, pub persistent_used: u64, pub task_used: u64,
    pub arena_chunks: u64, pub live_handles: u64, pub large_live_bytes: u64, pub large_live_blocks: u64,
    pub allocs: u64, pub promoted: u64, pub pool_free_chunks: u64, pub pool_hits: u64, pub pool_misses: u64,
}
#[no_mangle] pub unsafe extern "C" fn zs_mem_stats(rt: *mut Runtime, out: *mut ZMemStats) {
    let rt = &*rt; let s = rt.heap.stats(); let p = rt.zamm.pool().stats();
    *out = ZMemStats { frame_used: s.frame.used as u64, temp_used: s.temp.used as u64, persistent_used: s.persistent.used as u64, task_used: s.tasks.used as u64,
        arena_chunks: (s.frame.chunks + s.temp.chunks + s.persistent.chunks + s.tasks.chunks) as u64, live_handles: s.live_handles as u64,
        large_live_bytes: s.large_live_bytes as u64, large_live_blocks: s.large_live_blocks as u64, allocs: s.allocs, promoted: s.promoted,
        pool_free_chunks: p.0 as u64, pool_hits: p.2, pool_misses: p.3 };
}
