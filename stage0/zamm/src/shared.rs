use std::alloc::{alloc, dealloc, Layout};
use std::sync::Mutex;

/// Uchwyt do danych współdzielonych między wątkami. `Copy + Send + Sync`; zwolnienie przez `release`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SharedHandle { idx: u32, gen: u32 }

struct Slot { gen: u32, ptr: *mut u8, layout: Layout, refs: u32, dtor: Option<unsafe fn(*mut u8)>, live: bool }
unsafe impl Send for Slot {}

/// Globalna (na instancję `Zamm`) przestrzeń danych współdzielonych. Liczniki chronione mutexem;
/// wartość żyje, dopóki `refs > 0`. Dostęp do bajtów jest odpowiedzialnością trzymającego referencję.
pub struct SharedSpace { inner: Mutex<(Vec<Slot>, Vec<u32>)> }

impl SharedSpace {
    pub fn new() -> Self { SharedSpace { inner: Mutex::new((vec![], vec![])) } }

    /// Nowy blok (zerowany) z licznikiem 1. `dtor` zostanie wykonany przy spadku licznika do 0.
    pub fn alloc(&self, size: usize, align: usize, dtor: Option<unsafe fn(*mut u8)>) -> Option<SharedHandle> {
        let layout = Layout::from_size_align(size.max(1), align.max(1)).ok()?;
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() { return None; }
        unsafe { std::ptr::write_bytes(ptr, 0, layout.size()) };
        let mut g = self.inner.lock().unwrap();
        let (slots, free) = &mut *g;
        let s = Slot { gen: 0, ptr, layout, refs: 1, dtor, live: true };
        let idx = if let Some(i) = free.pop() { let gen = slots[i as usize].gen; slots[i as usize] = Slot { gen, ..s }; i }
                  else { slots.push(s); (slots.len() - 1) as u32 };
        Some(SharedHandle { idx, gen: slots[idx as usize].gen })
    }

    pub fn ptr(&self, h: SharedHandle) -> Option<*mut u8> {
        let g = self.inner.lock().unwrap();
        g.0.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen).map(|s| s.ptr)
    }

    pub fn retain(&self, h: SharedHandle) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.0.get_mut(h.idx as usize) { Some(s) if s.live && s.gen == h.gen => { s.refs += 1; true } _ => false }
    }

    /// Zwraca `true`, gdy to było ostatnie odwołanie i blok został zniszczony.
    pub fn release(&self, h: SharedHandle) -> bool {
        let victim = {
            let mut g = self.inner.lock().unwrap();
            let (slots, free) = &mut *g;
            let Some(s) = slots.get_mut(h.idx as usize).filter(|s| s.live && s.gen == h.gen) else { return false };
            s.refs -= 1;
            if s.refs > 0 { return false; }
            s.live = false; s.gen = s.gen.wrapping_add(1);
            free.push(h.idx);
            (s.ptr, s.layout, s.dtor.take())
        };
        // destruktor i zwolnienie poza blokadą (destruktor może zwalniać inne obiekty)
        unsafe { if let Some(d) = victim.2 { d(victim.0) } dealloc(victim.0, victim.1) }
        true
    }

    pub fn live_blocks(&self) -> usize { self.inner.lock().unwrap().0.iter().filter(|s| s.live).count() }
}

impl Drop for SharedSpace {
    fn drop(&mut self) {
        let g = self.inner.get_mut().unwrap();
        for s in g.0.iter_mut().filter(|s| s.live) { unsafe { if let Some(d) = s.dtor.take() { d(s.ptr) } dealloc(s.ptr, s.layout) } }
    }
}
