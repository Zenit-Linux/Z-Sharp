use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use zamm::*;

fn small() -> Zamm { let mut c = Config::default(); c.chunk_size = 1024; c.max_chunk_size = 8192; c.large_object = 4096; c.poison = true; Zamm::new(c) }

struct Noisy(&'static str, Rc<RefCell<Vec<&'static str>>>);
impl Drop for Noisy { fn drop(&mut self) { self.1.borrow_mut().push(self.0); } }

#[test]
fn arena_alignment_and_growth() {
    let z = small(); let a = z.arena();
    let mut ptrs = vec![];
    for i in 0..500usize {
        let align = 1usize << (i % 7);               // 1..64
        let size = 1 + (i * 37) % 200;
        let p = a.alloc_raw(size, align).unwrap().as_ptr();
        assert_eq!(p as usize % align, 0, "wyrównanie {align}");
        unsafe { std::ptr::write_bytes(p, (i % 251) as u8, size) };
        ptrs.push((p, size, (i % 251) as u8));
    }
    for (p, size, v) in ptrs { for k in 0..size { assert_eq!(unsafe { *p.add(k) }, v); } } // nic się nie nakłada
    let s = a.stats(); assert!(s.chunks > 1 && s.used > 0);
}

#[test]
fn oversized_request_gets_dedicated_chunk() {
    let z = small(); let a = z.arena();
    let p = a.alloc_raw(100_000, 8).unwrap().as_ptr();
    unsafe { std::ptr::write_bytes(p, 7, 100_000) };
    assert!(a.stats().reserved >= 100_000);
}

#[test]
fn rewind_runs_destructors_lifo_and_reuses_memory() {
    let z = small(); let mut a = z.arena();
    let log = Rc::new(RefCell::new(vec![]));
    a.alloc_val(Noisy("keep", log.clone()));
    let m = a.mark();
    let p1 = a.alloc_raw(64, 8).unwrap().as_ptr();
    a.alloc_val(Noisy("a", log.clone())); a.alloc_val(Noisy("b", log.clone())); a.alloc_val(Noisy("c", log.clone()));
    a.rewind(m);
    assert_eq!(*log.borrow(), vec!["c", "b", "a"]);
    let p2 = a.alloc_raw(64, 8).unwrap().as_ptr();
    assert_eq!(p1, p2, "pamięć po rewind jest ponownie używana");
    drop(a);
    assert_eq!(log.borrow().last(), Some(&"keep"));
}

#[test]
fn rewind_across_chunks_returns_chunks_to_pool() {
    let mut c = Config::default(); c.chunk_size = 512; c.max_chunk_size = 512; c.retain_chunks = 1; let z = Zamm::new(c);
    let mut a = z.arena();
    let m = a.mark();
    for _ in 0..40 { a.alloc_raw(256, 8).unwrap(); }
    assert!(a.stats().chunks >= 10);
    a.rewind(m);
    assert_eq!(a.stats().chunks, 1);
    assert!(z.pool().stats().0 >= 9, "chunki wróciły do puli");
    // kolejna arena korzysta z puli zamiast systemu
    let b = z.arena(); b.alloc_raw(100, 8).unwrap();
    assert!(z.pool().stats().2 >= 1, "trafienie w pulę");
}

#[test]
fn reset_retains_chunks_for_reuse() {
    let mut c = Config::default(); c.chunk_size = 256; c.max_chunk_size = 256; c.retain_chunks = 3; let z = Zamm::new(c);
    let mut a = z.arena();
    let first = a.alloc_raw(200, 8).unwrap().as_ptr();
    for _ in 0..10 { a.alloc_raw(200, 8).unwrap(); }
    a.reset();
    assert_eq!(a.stats().used, 0);
    assert!(a.stats().chunks <= 3 && a.stats().chunks >= 1);
    // po reset pierwsza alokacja wraca do zatrzymanego chunka (ten sam zestaw adresów)
    let again = a.alloc_raw(200, 8).unwrap().as_ptr();
    assert_eq!(first, again, "pierwsza alokacja po reset trafia w zatrzymany chunk");
}

#[test]
fn poison_marks_freed_memory() {
    let z = small(); let mut a = z.arena();
    let m = a.mark();
    let p = a.alloc_raw(32, 8).unwrap().as_ptr();
    unsafe { std::ptr::write_bytes(p, 1, 32) };
    a.rewind(m);
    assert_eq!(unsafe { *p }, 0xDD);
}

#[test]
fn strings_and_slices() {
    let z = small(); let a = z.arena();
    let s = a.alloc_str("zażółć gęślą jaźń");
    let v = a.alloc_slice_copy(&[1u32, 2, 3]);
    assert_eq!(s, "zażółć gęślą jaźń"); assert_eq!(v, &[1, 2, 3]);
}

#[test]
fn arena_transfer_between_threads() {
    let z = small(); let a = z.arena();
    let data = a.alloc_slice_copy(&[10u64, 20, 30]); let p = data.as_ptr() as usize;
    let t = a.into_transferable().map_err(|_| ()).unwrap();
    let h = std::thread::spawn(move || { let a = t.into_arena(); let s = unsafe { std::slice::from_raw_parts(p as *const u64, 3) }; (s.iter().sum::<u64>(), a.stats().used) });
    assert_eq!(h.join().unwrap().0, 60);
    // arena z wartością !Send nie przejdzie
    let b = z.arena(); b.alloc_val(Rc::new(1));
    assert!(b.into_transferable().is_err());
}

#[test]
fn frame_handles_go_stale_and_destructors_run() {
    let z = small(); let mut h = z.heap();
    let log = Rc::new(RefCell::new(vec![]));
    h.push_frame();
    let a = h.alloc_value(Lifetime::Frame, Noisy("a", log.clone())).unwrap();
    let b = h.alloc_value(Lifetime::Frame, Noisy("b", log.clone())).unwrap();
    assert!(h.get::<Noisy>(a).is_some());
    h.pop_frame().unwrap();
    assert_eq!(*log.borrow(), vec!["b", "a"]);
    assert!(h.get::<Noisy>(a).is_none() && h.get::<Noisy>(b).is_none(), "uchwyty nieaktualne");
    assert_eq!(h.pop_frame(), Err(Error::NoScope));
}

#[test]
fn referenced_object_blocks_scope_release() {
    let z = small(); let mut h = z.heap();
    let log = Rc::new(RefCell::new(vec![]));
    h.push_frame();
    let a = h.alloc_value(Lifetime::Frame, Noisy("a", log.clone())).unwrap();
    assert!(h.retain(a));
    assert_eq!(h.pop_frame(), Err(Error::Referenced(1)));
    assert!(log.borrow().is_empty() && h.get::<Noisy>(a).is_some(), "nic nie zwolniono");
    h.release(a);
    h.pop_frame().unwrap();
    assert_eq!(*log.borrow(), vec!["a"]);
}

#[test]
fn returned_value_moves_to_caller_and_drops_once() {
    let z = small(); let mut h = z.heap();
    let log = Rc::new(RefCell::new(vec![]));
    h.push_frame();                               // wywołujący
    h.push_frame();                               // wywołana
    let tmp = h.alloc_value(Lifetime::Frame, Noisy("tmp", log.clone())).unwrap();
    let ret = h.alloc_value(Lifetime::Frame, (Noisy("ret", log.clone()), 42u64)).unwrap();
    let out = h.pop_frame_returning(&[ret]).unwrap();
    assert_eq!(*log.borrow(), vec!["tmp"], "tylko tymczasowy zniszczony");
    assert!(!h.is_live(tmp) && !h.is_live(ret));
    assert_eq!(h.get::<(Noisy, u64)>(out[0]).unwrap().1, 42);
    h.pop_frame().unwrap();
    assert_eq!(*log.borrow(), vec!["tmp", "ret"], "ret zniszczony dokładnie raz, w ramce wywołującego");
}

#[test]
fn persistent_refcount_and_weak_handles() {
    let z = small(); let mut h = z.heap();
    let log = Rc::new(RefCell::new(vec![]));
    let strong = h.alloc_value(Lifetime::Persistent, Noisy("p", log.clone())).unwrap();
    let weak = strong;                              // Handle bez retain = słaby
    h.retain(strong);
    assert!(!h.release(strong));
    assert!(h.get::<Noisy>(weak).is_some());
    assert!(h.release(strong), "ostatnie odwołanie niszczy");
    assert_eq!(*log.borrow(), vec!["p"]);
    assert!(h.get::<Noisy>(weak).is_none(), "słaby uchwyt wygasa");
    assert!(!h.release(strong), "podwójne zwolnienie jest nieszkodliwe");
}

struct Node { name: &'static str, peer: Option<Handle>, log: Rc<RefCell<Vec<&'static str>>> }
impl Drop for Node { fn drop(&mut self) { self.log.borrow_mut().push(self.name); } }

#[test]
fn cycle_with_weak_edge_is_freed_without_gc() {
    let z = small(); let mut h = z.heap();
    let log = Rc::new(RefCell::new(vec![]));
    let a = h.alloc_value(Lifetime::Persistent, Node { name: "A", peer: None, log: log.clone() }).unwrap();
    let b = h.alloc_value(Lifetime::Persistent, Node { name: "B", peer: None, log: log.clone() }).unwrap();
    h.get_mut::<Node>(a).unwrap().peer = Some(b);   // silna krawędź A->B
    h.retain(b);
    h.get_mut::<Node>(b).unwrap().peer = Some(a);   // słaba krawędź B->A (bez retain)
    h.release(a);                                    // odpada właściciel A -> A zniszczony
    assert_eq!(*log.borrow(), vec!["A"]);
    assert!(h.get::<Node>(b).unwrap().peer.map_or(true, |p| h.get::<Node>(p).is_none()), "słaba krawędź nie ożywia A");
    h.release(b); h.release(b);
    assert_eq!(*log.borrow(), vec!["A", "B"]);
    assert_eq!(h.stats().live_handles, 0);
}

#[test]
fn large_objects_use_los_and_are_freed_with_scope() {
    let z = small(); let mut h = z.heap();
    h.push_frame();
    let big = h.alloc_layout(Lifetime::Frame, std::alloc::Layout::from_size_align(100_000, 16).unwrap(), None, None).unwrap();
    let p = h.ptr(big).unwrap(); unsafe { std::ptr::write_bytes(p, 5, 100_000) };
    let st = h.stats(); assert_eq!(st.large_live_blocks, 1); assert_eq!(st.large_live_bytes, 100_000);
    assert_eq!(st.frame.used, 0, "nie w arenie");
    h.pop_frame().unwrap();
    let st = h.stats(); assert_eq!((st.large_live_blocks, st.large_live_bytes, st.large_peak_bytes), (0, 0, 100_000));
}

#[test]
fn size_never_decides_lifetime() {
    let z = small(); let mut h = z.heap();
    h.push_frame();
    assert_eq!(h.classify(&Hint::new(1 << 20, Escape::No)), AllocClass::Local(Lifetime::Frame), "duży, ale lokalny");
    assert_eq!(h.classify(&Hint::new(8, Escape::Unknown)), AllocClass::Local(Lifetime::Persistent), "mały, ale nieudowodniony");
    assert_eq!(h.classify(&Hint::new(8, Escape::Program)), AllocClass::Local(Lifetime::Persistent));
    assert_eq!(h.classify(&Hint { size: 8, escape: Escape::No, shared: true, task: None }), AllocClass::Shared);
    h.pop_frame().unwrap();
    assert_eq!(h.classify(&Hint::new(8, Escape::No)), AllocClass::Local(Lifetime::Persistent), "bez ramki -> bezpiecznie");
}

#[test]
fn typed_handles_reject_type_confusion() {
    let z = small(); let mut h = z.heap();
    let a = h.alloc_value(Lifetime::Persistent, 7u64).unwrap();
    assert_eq!(h.get::<u64>(a), Some(&7)); assert!(h.get::<f64>(a).is_none());
}

#[test]
fn promote_moves_value_to_longer_lifetime() {
    let z = small(); let mut h = z.heap();
    let log = Rc::new(RefCell::new(vec![]));
    h.push_frame();
    let a = h.alloc_value(Lifetime::Frame, Noisy("a", log.clone())).unwrap();
    let p = h.promote(a, Lifetime::Persistent).unwrap();
    h.pop_frame().unwrap();
    assert!(log.borrow().is_empty(), "destruktor przeniesiony, nie wykonany");
    assert!(h.get::<Noisy>(p).is_some());
    h.release(p);
    assert_eq!(*log.borrow(), vec!["a"]);
}

#[test]
fn temp_scopes_nest_independently_of_frames() {
    let z = small(); let mut h = z.heap();
    h.push_frame(); h.begin_temp(); h.begin_temp();
    let t = h.alloc_value(Lifetime::Temp, 1u32).unwrap();
    h.end_temp().unwrap();
    assert!(!h.is_live(t));
    assert_eq!(h.stats().temp_depth, 1);
    h.end_temp().unwrap(); h.pop_frame().unwrap();
}

#[test]
fn tasks_have_own_arena_and_can_move_between_threads() {
    let z = small();
    let mut h = z.heap();
    let t = h.new_task();
    let s = h.task_arena(t).unwrap().alloc_slice_copy(&[1u64, 2, 3, 4]); let p = s.as_ptr() as usize;
    let det = h.detach_task(t).unwrap();
    assert_eq!(h.stats().live_tasks, 0);
    let z2 = z.clone();
    let r = std::thread::spawn(move || {
        let mut h2 = z2.heap();
        let t2 = h2.attach_task(det);
        let sum: u64 = unsafe { std::slice::from_raw_parts(p as *const u64, 4) }.iter().sum();
        h2.end_task(t2).unwrap(); sum
    }).join().unwrap();
    assert_eq!(r, 10);
    // zadanie z obiektem zarządzanym uchwytem nie może zostać odłączone
    let t = h.new_task(); h.alloc_value(Lifetime::Task(t), 1u8).unwrap();
    assert_eq!(h.detach_task(t).err(), Some(Error::NotTransferable));
    h.end_task(t).unwrap();
    assert_eq!(h.end_task(t), Err(Error::UnknownTask));
}

#[test]
fn shared_space_refcount_across_threads() {
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    unsafe fn dt(_p: *mut u8) { DROPS.fetch_add(1, Ordering::SeqCst); }
    let z = small();
    let sh = z.shared().alloc(64, 8, Some(dt)).unwrap();
    for _ in 0..8 { assert!(z.shared().retain(sh)); }
    let space: Arc<SharedSpace> = z.shared().clone();
    let ths: Vec<_> = (0..8).map(|_| { let s = space.clone(); std::thread::spawn(move || { assert!(s.ptr(sh).is_some()); s.release(sh); }) }).collect();
    for t in ths { t.join().unwrap(); }
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    assert!(space.release(sh), "ostatnia referencja niszczy");
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    assert!(space.ptr(sh).is_none() && !space.release(sh));
    assert_eq!(space.live_blocks(), 0);
}

#[test]
fn heap_drop_closes_everything_deterministically() {
    let z = small();
    let log = Rc::new(RefCell::new(vec![]));
    {
        let mut h = z.heap();
        h.push_frame();
        h.alloc_value(Lifetime::Frame, Noisy("frame", log.clone())).unwrap();
        h.alloc_value(Lifetime::Persistent, Noisy("p1", log.clone())).unwrap();
        h.alloc_value(Lifetime::Persistent, Noisy("p2", log.clone())).unwrap();
    }
    assert_eq!(*log.borrow(), vec!["frame", "p2", "p1"]);
}

#[test]
fn config_set_by_manifest_keys() {
    let mut c = Config::default();
    assert!(c.set("chunk-kib", 32) && c.chunk_size == 32 << 10);
    assert!(c.set("large-object-kib", 128) && c.large_object == 128 << 10);
    assert!(!c.set("nope", 1) && !c.set("chunk-kib", -5) && !c.set("chunk-kib", 0));
    assert!(c.set("max-chunk-kib", 4) && c.max_chunk_size >= c.chunk_size);
}

#[test]
fn stress_random_scopes() {
    let z = small(); let mut h = z.heap();
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut rnd = move || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
    let mut live: Vec<Vec<(Handle, u8, usize)>> = vec![];
    for _ in 0..3000 {
        match rnd() % 4 {
            0 if live.len() < 12 => { h.push_frame(); live.push(vec![]); }
            1 if !live.is_empty() => {
                let fr = live.pop().unwrap();
                for (hd, v, n) in &fr { let p = h.ptr(*hd).unwrap(); for k in 0..*n { assert_eq!(unsafe { *p.add(k) }, *v); } }
                h.pop_frame().unwrap();
                for (hd, _, _) in fr { assert!(!h.is_live(hd)); }
            }
            _ if !live.is_empty() => {
                let n = 1 + (rnd() % 6000) as usize; let v = (rnd() % 255) as u8;
                let hd = h.alloc_layout(Lifetime::Frame, std::alloc::Layout::from_size_align(n, 8).unwrap(), None, None).unwrap();
                unsafe { std::ptr::write_bytes(h.ptr(hd).unwrap(), v, n) };
                live.last_mut().unwrap().push((hd, v, n));
            }
            _ => {}
        }
    }
    while !live.is_empty() { live.pop(); h.pop_frame().unwrap(); }
    let s = h.stats(); assert_eq!((s.live_handles, s.large_live_blocks), (0, 0));
}
