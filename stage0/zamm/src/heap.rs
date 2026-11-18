use crate::arena::{Arena, ArenaStats, Mark, Transferable};
use crate::Zamm;
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::any::TypeId;
use std::collections::HashMap;

pub type DropFn = unsafe fn(*mut u8);
unsafe fn glue<T>(p: *mut u8) { std::ptr::drop_in_place(p as *mut T) }
/// Destruktor typu `T` jako wskaźnik funkcji (`None`, jeśli `T` nie wymaga niszczenia).
pub fn drop_fn_of<T>() -> Option<DropFn> { if std::mem::needs_drop::<T>() { Some(glue::<T>) } else { None } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

/// Czas życia alokacji (magazyn — arena lub LOS — dobiera rozmiar, nie ta wartość).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifetime { Frame, Temp, Task(TaskId), Persistent }

/// Wynik analizy ucieczki (escape analysis) dostarczony przez kompilator lub przybliżony przez runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escape {
    /// wartość nie opuszcza ramki wywołania
    No,
    /// wartość żyje najwyżej do końca bieżącej instrukcji/wyrażenia
    Statement,
    /// wartość jest zwracana do wywołującego (przenoszona przy `pop_frame_returning`)
    Caller,
    /// wartość należy do zadania
    Task,
    /// wartość żyje do końca programu
    Program,
    /// analiza nie udowodniła nic — bezpieczny wybór: trwała, własność przez licznik
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct Hint { pub size: usize, pub escape: Escape, pub shared: bool, pub task: Option<TaskId> }
impl Hint { pub fn new(size: usize, escape: Escape) -> Hint { Hint { size, escape, shared: false, task: None } } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocClass { Local(Lifetime), Shared }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
    /// brak otwartej ramki / zakresu tymczasowego
    NoScope,
    UnknownTask,
    /// zakres nie może zostać zwolniony: istnieją legalne odwołania (liczba obiektów z `refs > 0`)
    Referenced(usize),
    /// uchwyt nieaktualny (obiekt zwolniony) lub z innego zakresu
    Stale,
    /// zadanie zawiera obiekty zarządzane uchwytami lub wartości związane z wątkiem
    NotTransferable,
    BadLayout,
}
impl std::fmt::Display for Error { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") } }
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle { idx: u32, gen: u32 }
impl Handle { pub fn raw(self) -> u64 { ((self.gen as u64) << 32) | self.idx as u64 } pub fn from_raw(r: u64) -> Handle { Handle { idx: r as u32, gen: (r >> 32) as u32 } } }

#[derive(Clone, Copy, PartialEq, Eq, Debug)] enum Store { Arena, Los }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] enum Owner { Frame, Temp, Task, Persistent }

struct Slot { gen: u32, live: bool, ptr: *mut u8, layout: Layout, store: Store, dtor: Option<DropFn>, refs: u32, owner: Owner, ty: Option<TypeId> }

#[derive(Default)]
struct Scope { items: Vec<Handle> }
struct Stack { arena: Arena, marks: Vec<(Mark, Scope)> }

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub frame: ArenaStats, pub temp: ArenaStats, pub persistent: ArenaStats, pub tasks: ArenaStats,
    pub frame_depth: usize, pub temp_depth: usize, pub live_tasks: usize,
    pub live_handles: usize, pub large_live_bytes: usize, pub large_live_blocks: usize, pub large_peak_bytes: usize,
    pub allocs: u64, pub promoted: u64,
}

pub struct Heap {
    zamm: Zamm,
    frames: Stack,
    temps: Stack,
    tasks: HashMap<TaskId, (Arena, Scope)>,
    persistent: (Arena, Scope),
    slots: Vec<Slot>,
    free: Vec<u32>,
    next_task: u64,
    large_bytes: usize, large_blocks: usize, large_peak: usize,
    allocs: u64, promoted: u64,
}

impl Heap {
    pub fn new(zamm: Zamm) -> Heap {
        Heap { frames: Stack { arena: zamm.arena(), marks: vec![] }, temps: Stack { arena: zamm.arena(), marks: vec![] },
               tasks: HashMap::new(), persistent: (zamm.arena(), Scope::default()), slots: vec![], free: vec![], next_task: 1,
               large_bytes: 0, large_blocks: 0, large_peak: 0, allocs: 0, promoted: 0, zamm }
    }
    pub fn zamm(&self) -> &Zamm { &self.zamm }

    // ───────────── adaptacyjny wybór klasy ─────────────
    /// Dobiera klasę z dowodu ucieczki. Rozmiar NIE wpływa na czas życia (tylko na magazyn).
    pub fn classify(&self, h: &Hint) -> AllocClass {
        if h.shared { return AllocClass::Shared; }
        let frame_ok = !self.frames.marks.is_empty();
        let temp_ok = !self.temps.marks.is_empty();
        AllocClass::Local(match h.escape {
            Escape::No | Escape::Caller if frame_ok => Lifetime::Frame,
            Escape::Statement if temp_ok => Lifetime::Temp,
            Escape::Statement if frame_ok => Lifetime::Frame,
            Escape::Task => match h.task { Some(t) if self.tasks.contains_key(&t) => Lifetime::Task(t), _ => Lifetime::Persistent },
            _ => Lifetime::Persistent,
        })
    }

    // ───────────── zakresy ─────────────
    pub fn push_frame(&mut self) { let m = self.frames.arena.mark(); self.frames.marks.push((m, Scope::default())); }
    pub fn frame_depth(&self) -> usize { self.frames.marks.len() }
    pub fn begin_temp(&mut self) { let m = self.temps.arena.mark(); self.temps.marks.push((m, Scope::default())); }
    pub fn temp_depth(&self) -> usize { self.temps.marks.len() }

    /// Zamyka ramkę: niszczy obiekty w odwrotnej kolejności i zwalnia pamięć.
    /// Jeśli któryś obiekt ma legalne odwołania (`retain`), zwraca `Referenced(n)` i NIC nie zwalnia.
    pub fn pop_frame(&mut self) -> Result<(), Error> { self.pop_frame_returning(&[]).map(|_| ()) }
    pub fn end_temp(&mut self) -> Result<(), Error> { self.end_stack(true, &[]).map(|_| ()) }

    /// Zamyka ramkę, przenosząc wartości `rets` do ramki wywołującego (albo do areny trwałej na dole stosu).
    /// To jest formalny transfer własności „zwracany obiekt przeżywa funkcję”.
    pub fn pop_frame_returning(&mut self, rets: &[Handle]) -> Result<Vec<Handle>, Error> { self.end_stack(false, rets) }

    fn end_stack(&mut self, temp: bool, rets: &[Handle]) -> Result<Vec<Handle>, Error> {
        let stack = if temp { &self.temps } else { &self.frames };
        let Some((_, scope)) = stack.marks.last() else { return Err(Error::NoScope) };
        let owner = if temp { Owner::Temp } else { Owner::Frame };
        for r in rets { if !self.is_live(*r) || self.slots[r.idx as usize].owner != owner { return Err(Error::Stale); } }
        let blocked = scope.items.iter().filter(|h| { let s = &self.slots[h.idx as usize]; s.live && s.gen == h.gen && s.refs > 0 && !rets.contains(h) }).count();
        if blocked > 0 { return Err(Error::Referenced(blocked)); }
        // zachowujemy bajty zwracanych wartości, bo ich pamięć zniknie z ramką
        let mut moved: Vec<(Vec<u8>, Layout, Option<DropFn>, Option<TypeId>)> = vec![];
        for r in rets {
            let s = &mut self.slots[r.idx as usize];
            let bytes = unsafe { std::slice::from_raw_parts(s.ptr, s.layout.size()).to_vec() };
            moved.push((bytes, s.layout, s.dtor.take(), s.ty));
        }
        let stack = if temp { &mut self.temps } else { &mut self.frames };
        let (mark, scope) = stack.marks.pop().unwrap();
        self.destroy_scope(scope);
        let stack = if temp { &mut self.temps } else { &mut self.frames };
        stack.arena.rewind(mark);
        let mut out = vec![];
        for (bytes, layout, dtor, ty) in moved {
            let target = if temp { self.classify(&Hint::new(layout.size(), Escape::Statement)) }
                         else { self.classify(&Hint::new(layout.size(), Escape::Caller)) };
            let AllocClass::Local(lt) = target else { unreachable!() };
            let h = self.alloc_layout(lt, layout, dtor, ty)?;
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.slots[h.idx as usize].ptr, bytes.len()) };
            self.promoted += 1;
            out.push(h);
        }
        Ok(out)
    }

    pub fn new_task(&mut self) -> TaskId {
        let id = TaskId(self.next_task); self.next_task += 1;
        self.tasks.insert(id, (self.zamm.arena(), Scope::default()));
        id
    }
    pub fn task_arena(&self, id: TaskId) -> Option<&Arena> { self.tasks.get(&id).map(|t| &t.0) }

    /// Kończy zadanie: niszczy jego obiekty i oddaje chunki do puli.
    pub fn end_task(&mut self, id: TaskId) -> Result<(), Error> {
        let (_, scope) = self.tasks.get(&id).ok_or(Error::UnknownTask)?;
        let blocked = scope.items.iter().filter(|h| { let s = &self.slots[h.idx as usize]; s.live && s.gen == h.gen && s.refs > 0 }).count();
        if blocked > 0 { return Err(Error::Referenced(blocked)); }
        let (arena, scope) = self.tasks.remove(&id).unwrap();
        self.destroy_scope(scope);
        drop(arena);
        Ok(())
    }

    /// Odłącza arenę zadania do przekazania innemu wątkowi (własność przechodzi, bez kopiowania).
    /// Wymaga, by zadanie nie miało obiektów zarządzanych uchwytami i nie zawierało wartości `!Send`.
    pub fn detach_task(&mut self, id: TaskId) -> Result<Transferable, Error> {
        let (_, scope) = self.tasks.get(&id).ok_or(Error::UnknownTask)?;
        if scope.items.iter().any(|h| self.is_live(*h)) { return Err(Error::NotTransferable); }
        let (arena, _) = self.tasks.remove(&id).unwrap();
        arena.into_transferable().map_err(|a| { self.tasks.insert(id, (a, Scope::default())); Error::NotTransferable })
    }
    pub fn attach_task(&mut self, t: Transferable) -> TaskId {
        let id = TaskId(self.next_task); self.next_task += 1;
        self.tasks.insert(id, (t.into_arena(), Scope::default()));
        id
    }

    // ───────────── alokacja ─────────────
    /// Alokacja zerowana, zarządzana uchwytem. Obiekty `Persistent` startują z `refs = 1` (właściciel);
    /// pozostałe z `refs = 0` (należą do zakresu).
    pub fn alloc_layout(&mut self, lt: Lifetime, layout: Layout, dtor: Option<DropFn>, ty: Option<TypeId>) -> Result<Handle, Error> {
        let large = layout.size() >= self.zamm.cfg.large_object;
        let (owner, arena): (Owner, &Arena) = match lt {
            Lifetime::Frame => { if self.frames.marks.is_empty() { return Err(Error::NoScope) } (Owner::Frame, &self.frames.arena) }
            Lifetime::Temp => { if self.temps.marks.is_empty() { return Err(Error::NoScope) } (Owner::Temp, &self.temps.arena) }
            Lifetime::Task(id) => (Owner::Task, &self.tasks.get(&id).ok_or(Error::UnknownTask)?.0),
            Lifetime::Persistent => (Owner::Persistent, &self.persistent.0),
        };
        let (ptr, store) = if large {
            let p = unsafe { alloc_zeroed(layout) };
            if p.is_null() { return Err(Error::OutOfMemory); }
            self.large_bytes += layout.size(); self.large_blocks += 1; self.large_peak = self.large_peak.max(self.large_bytes);
            (p, Store::Los)
        } else {
            let p = arena.alloc_raw(layout.size(), layout.align()).ok_or(Error::OutOfMemory)?.as_ptr();
            if layout.size() > 0 { unsafe { std::ptr::write_bytes(p, 0, layout.size()) } }
            (p, Store::Arena)
        };
        let slot = Slot { gen: 0, live: true, ptr, layout, store, dtor, refs: if owner == Owner::Persistent { 1 } else { 0 }, owner, ty };
        let idx = match self.free.pop() { Some(i) => { let gen = self.slots[i as usize].gen; self.slots[i as usize] = Slot { gen, ..slot }; i }
                                          None => { self.slots.push(slot); (self.slots.len() - 1) as u32 } };
        let h = Handle { idx, gen: self.slots[idx as usize].gen };
        match lt {
            Lifetime::Frame => self.frames.marks.last_mut().unwrap().1.items.push(h),
            Lifetime::Temp => self.temps.marks.last_mut().unwrap().1.items.push(h),
            Lifetime::Task(id) => self.tasks.get_mut(&id).unwrap().1.items.push(h),
            Lifetime::Persistent => self.persistent.1.items.push(h),
        }
        self.allocs += 1;
        Ok(h)
    }

    /// Alokuje wartość typu `T` (niszczoną deterministycznie przy końcu zakresu lub `release`).
    pub fn alloc_value<T: 'static>(&mut self, lt: Lifetime, v: T) -> Result<Handle, Error> {
        let h = self.alloc_layout(lt, Layout::new::<T>(), drop_fn_of::<T>(), Some(TypeId::of::<T>()))?;
        unsafe { (self.slots[h.idx as usize].ptr as *mut T).write(v) };
        Ok(h)
    }

    /// Alokacja z podpowiedzi analizatora: klasa wybrana przez `classify`. Dla `shared` użyj `Zamm::shared()`.
    pub fn alloc_hinted<T: 'static>(&mut self, hint: Hint, v: T) -> Result<Handle, Error> {
        match self.classify(&hint) { AllocClass::Local(lt) => self.alloc_value(lt, v), AllocClass::Shared => Err(Error::BadLayout) }
    }

    /// Szybka ścieżka dla danych o udowodnionym czasie życia: bez uchwytu i bez licznika.
    /// Duże bloki także dostają uchwyt (LOS wymaga śledzenia) — wskaźnik jest ważny do końca zakresu.
    pub fn alloc_in(&mut self, lt: Lifetime, size: usize, align: usize) -> Result<*mut u8, Error> {
        let layout = Layout::from_size_align(size, align).map_err(|_| Error::BadLayout)?;
        if size < self.zamm.cfg.large_object {
            let arena = match lt {
                Lifetime::Frame => { if self.frames.marks.is_empty() { return Err(Error::NoScope) } &self.frames.arena }
                Lifetime::Temp => { if self.temps.marks.is_empty() { return Err(Error::NoScope) } &self.temps.arena }
                Lifetime::Task(id) => &self.tasks.get(&id).ok_or(Error::UnknownTask)?.0,
                Lifetime::Persistent => &self.persistent.0,
            };
            let p = arena.alloc_raw(size, align).ok_or(Error::OutOfMemory)?.as_ptr();
            if size > 0 { unsafe { std::ptr::write_bytes(p, 0, size) } }
            return Ok(p);
        }
        let h = self.alloc_layout(lt, layout, None, None)?;
        Ok(self.slots[h.idx as usize].ptr)
    }

    // ───────────── uchwyty ─────────────
    pub fn is_live(&self, h: Handle) -> bool { self.slots.get(h.idx as usize).map_or(false, |s| s.live && s.gen == h.gen) }
    pub fn ptr(&self, h: Handle) -> Option<*mut u8> { self.slots.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen).map(|s| s.ptr) }
    pub fn layout(&self, h: Handle) -> Option<Layout> { self.slots.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen).map(|s| s.layout) }
    pub fn get<T: 'static>(&self, h: Handle) -> Option<&T> {
        let s = self.slots.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen)?;
        if s.ty.map_or(false, |t| t != TypeId::of::<T>()) { return None; }
        Some(unsafe { &*(s.ptr as *const T) })
    }
    pub fn get_mut<T: 'static>(&mut self, h: Handle) -> Option<&mut T> {
        let s = self.slots.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen)?;
        if s.ty.map_or(false, |t| t != TypeId::of::<T>()) { return None; }
        Some(unsafe { &mut *(s.ptr as *mut T) })
    }
    pub fn refs(&self, h: Handle) -> Option<u32> { self.slots.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen).map(|s| s.refs) }

    /// Silne odwołanie: blokuje zwolnienie zakresu, dopóki nie zostanie `release`.
    pub fn retain(&mut self, h: Handle) -> bool {
        match self.slots.get_mut(h.idx as usize).filter(|s| s.live && s.gen == h.gen) { Some(s) => { s.refs += 1; true } None => false }
    }
    /// Zwalnia silne odwołanie. Dla obiektów `Persistent` spadek do 0 niszczy obiekt natychmiast
    /// (słabe uchwyty — zwykłe kopie `Handle` — przestają działać: `get` zwraca `None`). Zwraca `true`, gdy zniszczono.
    pub fn release(&mut self, h: Handle) -> bool {
        let Some(s) = self.slots.get_mut(h.idx as usize).filter(|s| s.live && s.gen == h.gen) else { return false };
        if s.refs == 0 { return false; }
        s.refs -= 1;
        if s.refs == 0 && s.owner == Owner::Persistent { self.destroy(h); return true; }
        false
    }

    /// Przenosi wartość do innego czasu życia (bitowy ruch + przekazanie destruktora).
    /// Obiekty wskazujące na siebie przez `Handle` (nie surowe wskaźniki) pozostają poprawne.
    pub fn promote(&mut self, h: Handle, to: Lifetime) -> Result<Handle, Error> {
        let s = self.slots.get(h.idx as usize).filter(|s| s.live && s.gen == h.gen).ok_or(Error::Stale)?;
        let (layout, dtor, ty, src) = (s.layout, s.dtor, s.ty, s.ptr);
        let nh = self.alloc_layout(to, layout, dtor, ty)?;
        unsafe { std::ptr::copy_nonoverlapping(src, self.slots[nh.idx as usize].ptr, layout.size()) };
        self.slots[h.idx as usize].dtor = None;
        self.promoted += 1;
        Ok(nh)
    }

    fn destroy(&mut self, h: Handle) {
        let Some(s) = self.slots.get_mut(h.idx as usize).filter(|s| s.live && s.gen == h.gen) else { return };
        let (ptr, layout, store, dtor) = (s.ptr, s.layout, s.store, s.dtor.take());
        s.live = false; s.gen = s.gen.wrapping_add(1); s.refs = 0;
        self.free.push(h.idx);
        unsafe { if let Some(d) = dtor { d(ptr) } }
        if store == Store::Los { unsafe { dealloc(ptr, layout) } self.large_bytes -= layout.size(); self.large_blocks -= 1; }
    }

    fn destroy_scope(&mut self, scope: Scope) { for h in scope.items.into_iter().rev() { self.destroy(h); } }

    pub fn stats(&self) -> Stats {
        let mut t = ArenaStats::default();
        for (a, _) in self.tasks.values() { let s = a.stats(); t.chunks += s.chunks; t.reserved += s.reserved; t.used += s.used; t.destructors += s.destructors; }
        Stats { frame: self.frames.arena.stats(), temp: self.temps.arena.stats(), persistent: self.persistent.0.stats(), tasks: t,
                frame_depth: self.frames.marks.len(), temp_depth: self.temps.marks.len(), live_tasks: self.tasks.len(),
                live_handles: self.slots.iter().filter(|s| s.live).count(), large_live_bytes: self.large_bytes,
                large_live_blocks: self.large_blocks, large_peak_bytes: self.large_peak, allocs: self.allocs, promoted: self.promoted }
    }
}

impl Drop for Heap {
    /// Deterministyczne zamknięcie: ramki, zakresy tymczasowe, zadania, potem obiekty trwałe (LIFO).
    fn drop(&mut self) {
        while let Some((m, sc)) = self.frames.marks.pop() { self.destroy_scope(sc); self.frames.arena.rewind(m); }
        while let Some((m, sc)) = self.temps.marks.pop() { self.destroy_scope(sc); self.temps.arena.rewind(m); }
        let ids: Vec<TaskId> = self.tasks.keys().copied().collect();
        for id in ids { if let Some((_a, sc)) = self.tasks.remove(&id) { self.destroy_scope(sc); } }
        let sc = std::mem::take(&mut self.persistent.1);
        self.destroy_scope(sc);
    }
}
