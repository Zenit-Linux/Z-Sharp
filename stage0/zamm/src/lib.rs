mod arena;
mod chunk;
mod config;
mod heap;
mod shared;

pub use arena::{Arena, ArenaStats, Mark, Transferable};
pub use chunk::ChunkPool;
pub use config::Config;
pub use heap::{drop_fn_of, AllocClass, DropFn, Error, Escape, Handle, Heap, Hint, Lifetime, Stats, TaskId};
pub use shared::{SharedHandle, SharedSpace};

use std::sync::Arc;

/// Wspólny dla procesu zbiór zasobów ZAMM (pula chunków + przestrzeń współdzielona).
/// `Zamm` jest `Send + Sync`; każdy wątek tworzy własny [`Heap`] przez [`Zamm::heap`].
#[derive(Clone)]
pub struct Zamm {
    pub(crate) cfg: Arc<Config>,
    pub(crate) pool: Arc<ChunkPool>,
    pub(crate) shared: Arc<SharedSpace>,
}

impl Zamm {
    pub fn new(cfg: Config) -> Zamm {
        let cfg = Arc::new(cfg);
        Zamm { pool: Arc::new(ChunkPool::new(cfg.clone())), shared: Arc::new(SharedSpace::new()), cfg }
    }
    pub fn config(&self) -> &Config { &self.cfg }
    pub fn pool(&self) -> &Arc<ChunkPool> { &self.pool }
    pub fn shared(&self) -> &Arc<SharedSpace> { &self.shared }
    pub fn heap(&self) -> Heap { Heap::new(self.clone()) }
    pub fn arena(&self) -> Arena { Arena::new(self.pool.clone()) }
}
