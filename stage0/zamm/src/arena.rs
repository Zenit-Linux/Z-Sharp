use crate::chunk::{Chunk, ChunkPool};
use std::alloc::Layout;
use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::Arc;

type Dtor = (*mut u8, unsafe fn(*mut u8));

struct State {
    chunks: Vec<Chunk>,
    idx: usize,          // chunk, z którego aktualnie alokujemy
    cur: usize,          // offset w chunks[idx]
    next_size: usize,
    dtors: Vec<Dtor>,
    used: usize,         // bajty wydane użytkownikowi
    thread_bound: bool,  // zawiera wartości !Send -> nie można przenieść do innego wątku
}

/// Chained arena: alokacja przez przesunięcie wskaźnika, chunki dołączane w miarę potrzeby.
/// `!Sync` i domyślnie `!Send` (patrz [`Arena::into_transferable`]).
pub struct Arena { pool: Arc<ChunkPool>, st: RefCell<State> }

/// Znacznik do `rewind` (zwolnienie wszystkiego, co zaalokowano po nim).
#[derive(Clone, Copy, Debug)]
pub struct Mark { idx: usize, cur: usize, dtors: usize, used: usize }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaStats { pub chunks: usize, pub reserved: usize, pub used: usize, pub destructors: usize }

/// Arena gotowa do przekazania innemu wątkowi (tylko gdy nie zawiera wartości `!Send`).
pub struct Transferable(Arena);
unsafe impl Send for Transferable {}
impl Transferable { pub fn into_arena(self) -> Arena { self.0 } }

#[inline] fn align_up(x: usize, a: usize) -> usize { (x + a - 1) & !(a - 1) }

impl Arena {
    pub fn new(pool: Arc<ChunkPool>) -> Arena {
        let first = pool.config().chunk_size;
        Arena { pool, st: RefCell::new(State { chunks: vec![], idx: 0, cur: 0, next_size: first, dtors: vec![], used: 0, thread_bound: false }) }
    }

    /// Surowa alokacja. Zwraca `None` tylko przy braku pamięci lub przepełnieniu rozmiaru.
    pub fn alloc_raw(&self, size: usize, align: usize) -> Option<NonNull<u8>> {
        assert!(align.is_power_of_two(), "wyrównanie musi być potęgą dwójki");
        if size == 0 { return NonNull::new(align as *mut u8); }
        let mut st = self.st.borrow_mut();
        // 1) bieżący chunk, 2) kolejne zatrzymane chunki (po reset/rewind), 3) nowy chunk
        while st.idx < st.chunks.len() {
            let c = &st.chunks[st.idx];
            let base = c.base() as usize;
            let start = align_up(base + st.cur, align);
            if let Some(end) = start.checked_add(size) {
                if end <= base + c.size() {
                    st.cur = end - base; st.used += size;
                    return NonNull::new(start as *mut u8);
                }
            }
            if st.idx + 1 < st.chunks.len() { st.idx += 1; st.cur = 0; } else { break; }
        }
        let need = size.checked_add(align)?.checked_add(16)?;
        let want = need.max(st.next_size);
        let chunk = self.pool.take(want)?;
        let cfg = self.pool.config();
        st.next_size = st.next_size.saturating_mul(2).min(cfg.max_chunk_size).max(cfg.chunk_size);
        let base = chunk.base() as usize;
        let start = align_up(base, align);
        st.cur = start + size - base; st.used += size;
        st.chunks.push(chunk);
        st.idx = st.chunks.len() - 1;
        NonNull::new(start as *mut u8)
    }

    /// Alokuje wartość; jeśli `T` ma destruktor, zostanie on wykonany (LIFO) przy `reset`/`rewind`/drop.
    /// Oznacza arenę jako związaną z wątkiem (T może być `!Send`). Dla `Send` użyj [`Arena::alloc_send`].
    pub fn alloc_val<T>(&self, v: T) -> &mut T { self.st.borrow_mut().thread_bound = true; unsafe { self.place(v) } }

    /// Jak `alloc_val`, ale wymaga `T: Send`, więc arena pozostaje przenośna między wątkami.
    pub fn alloc_send<T: Send>(&self, v: T) -> &mut T { unsafe { self.place(v) } }

    unsafe fn place<T>(&self, v: T) -> &mut T {
        let l = Layout::new::<T>();
        let p = self.alloc_raw(l.size(), l.align()).expect("ZAMM: brak pamięci").as_ptr() as *mut T;
        p.write(v);
        if std::mem::needs_drop::<T>() { self.st.borrow_mut().dtors.push((p as *mut u8, drop_glue::<T>)); }
        &mut *p
    }

    pub fn alloc_slice_copy<T: Copy + Send>(&self, src: &[T]) -> &mut [T] {
        let l = Layout::array::<T>(src.len()).expect("rozmiar");
        let p = self.alloc_raw(l.size(), l.align()).expect("ZAMM: brak pamięci").as_ptr() as *mut T;
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), p, src.len()); std::slice::from_raw_parts_mut(p, src.len()) }
    }

    pub fn alloc_str(&self, s: &str) -> &str {
        let b = self.alloc_slice_copy(s.as_bytes());
        unsafe { std::str::from_utf8_unchecked(b) }
    }

    pub fn mark(&self) -> Mark {
        let st = self.st.borrow();
        Mark { idx: st.idx, cur: st.cur, dtors: st.dtors.len(), used: st.used }
    }

    /// Zwalnia wszystko po `m`: najpierw destruktory (odwrotnie do kolejności alokacji), potem pamięć.
    /// `&mut self` gwarantuje brak żywych referencji do zwalnianych danych.
    pub fn rewind(&mut self, m: Mark) { self.run_dtors(m.dtors); self.release_to(m.idx, m.cur, m.used); }

    fn release_to(&mut self, idx: usize, cur: usize, used: usize) {
        let (retain, poison) = { let c = self.pool.config(); (c.retain_chunks, c.poison) };
        let st = self.st.get_mut();
        if poison {
            let last = st.idx.min(st.chunks.len().saturating_sub(1));
            for i in idx..=last {
                if let Some(c) = st.chunks.get(i) {
                    let from = if i == idx { cur.min(c.size()) } else { 0 };
                    unsafe { std::ptr::write_bytes(c.base().add(from), 0xDD, c.size() - from) }
                }
            }
        }
        st.idx = idx; st.cur = cur; st.used = used;
        // nadmiarowe chunki (ponad `retain` za bieżącym) wracają do wspólnej puli
        let keep = (idx + 1).max(retain).min(st.chunks.len());
        while st.chunks.len() > keep { let c = st.chunks.pop().unwrap(); self.pool.give(c); }
    }

    /// Zwalnia całą zawartość; zatrzymuje do `retain_chunks` największych chunków do ponownego użycia.
    pub fn reset(&mut self) {
        self.run_dtors(0);
        let first = self.pool.config().chunk_size;
        self.st.get_mut().chunks.sort_by_key(|c| std::cmp::Reverse(c.size()));
        self.release_to(0, 0, 0);
        self.st.get_mut().next_size = first;
    }

    fn run_dtors(&mut self, down_to: usize) {
        loop {
            let d = { let st = self.st.get_mut(); if st.dtors.len() <= down_to { break } st.dtors.pop().unwrap() };
            unsafe { (d.1)(d.0) }
        }
    }

    pub fn stats(&self) -> ArenaStats {
        let st = self.st.borrow();
        ArenaStats { chunks: st.chunks.len(), reserved: st.chunks.iter().map(|c| c.size()).sum(), used: st.used, destructors: st.dtors.len() }
    }

    /// Przekazanie własności areny innemu wątkowi. Zwraca `Err(self)`, jeśli zawiera wartości związane z wątkiem.
    pub fn into_transferable(self) -> Result<Transferable, Arena> {
        if self.st.borrow().thread_bound { Err(self) } else { Ok(Transferable(self)) }
    }
}

unsafe fn drop_glue<T>(p: *mut u8) { std::ptr::drop_in_place(p as *mut T) }

impl Drop for Arena {
    fn drop(&mut self) {
        self.run_dtors(0);
        let st = self.st.get_mut();
        while let Some(c) = st.chunks.pop() { self.pool.give(c); }
    }
}
