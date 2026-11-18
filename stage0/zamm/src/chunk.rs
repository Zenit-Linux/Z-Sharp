use crate::config::Config;
use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

/// Bazowe wyrównanie każdego chunka.
pub const CHUNK_ALIGN: usize = 16;

pub struct Chunk { ptr: NonNull<u8>, size: usize }
unsafe impl Send for Chunk {}

impl Chunk {
    pub fn new(size: usize) -> Option<Chunk> {
        let layout = Layout::from_size_align(size.max(CHUNK_ALIGN), CHUNK_ALIGN).ok()?;
        let ptr = NonNull::new(unsafe { alloc(layout) })?;
        Some(Chunk { ptr, size: layout.size() })
    }
    #[inline] pub fn base(&self) -> *mut u8 { self.ptr.as_ptr() }
    #[inline] pub fn size(&self) -> usize { self.size }
}
impl Drop for Chunk {
    fn drop(&mut self) { unsafe { dealloc(self.ptr.as_ptr(), Layout::from_size_align_unchecked(self.size, CHUNK_ALIGN)) } }
}

#[derive(Default)]
struct PoolInner { free: Vec<Chunk>, bytes: usize, hits: u64, misses: u64 }

/// Współdzielona pula wolnych chunków (ponowne użycie chunków między arenami i wątkami).
pub struct ChunkPool { cfg: Arc<Config>, inner: Mutex<PoolInner> }

impl ChunkPool {
    pub fn new(cfg: Arc<Config>) -> ChunkPool { ChunkPool { cfg, inner: Mutex::new(PoolInner::default()) } }
    pub fn config(&self) -> &Config { &self.cfg }

    /// Zwraca chunk o rozmiarze >= `min` (najmniejszy pasujący z puli, inaczej świeży).
    pub fn take(&self, min: usize) -> Option<Chunk> {
        {
            let mut g = self.inner.lock().unwrap();
            let best = g.free.iter().enumerate().filter(|(_, c)| c.size() >= min).min_by_key(|(_, c)| c.size()).map(|(i, _)| i);
            if let Some(i) = best { let c = g.free.swap_remove(i); g.bytes -= c.size(); g.hits += 1; return Some(c); }
            g.misses += 1;
        }
        Chunk::new(min)
    }

    /// Oddaje chunk; jeśli pula jest pełna, pamięć wraca do systemu.
    pub fn give(&self, c: Chunk) {
        let mut g = self.inner.lock().unwrap();
        if g.bytes + c.size() <= self.cfg.pool_cap_bytes { g.bytes += c.size(); g.free.push(c); }
    }

    /// (wolne chunki, wolne bajty, trafienia, chybienia)
    pub fn stats(&self) -> (usize, usize, u64, u64) { let g = self.inner.lock().unwrap(); (g.free.len(), g.bytes, g.hits, g.misses) }
    pub fn trim(&self) { let mut g = self.inner.lock().unwrap(); g.free.clear(); g.bytes = 0; }
}
