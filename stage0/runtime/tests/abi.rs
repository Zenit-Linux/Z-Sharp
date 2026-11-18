use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use zs_runtime::*;

static CLOSED: AtomicUsize = AtomicUsize::new(0);
extern "C" fn close_cb(_c: *mut c_void) { CLOSED.fetch_add(1, Ordering::SeqCst); }

#[test]
fn strings_arrays_in_frames() {
    unsafe {
        let rt = zs_runtime_new(1, 0);
        zs_frame_push(rt);
        let (mut a, mut b, mut c) = (ZValue::NULL, ZValue::NULL, ZValue::NULL);
        assert_eq!(zs_str_new(rt, ZS_FRAME as i64, b"Hello, ".as_ptr(), 7, &mut a), ZS_OK);
        assert_eq!(zs_str_new(rt, ZS_FRAME as i64, "Z# żółw".as_ptr(), "Z# żółw".len(), &mut b), ZS_OK);
        assert_eq!(zs_str_concat(rt, ZS_FRAME as i64, a, b, &mut c), ZS_OK);
        let s = std::slice::from_raw_parts(zs_str_ptr(c), zs_str_len(c) as usize);
        assert_eq!(std::str::from_utf8(s).unwrap(), "Hello, Z# żółw");
        assert_eq!(zs_str_concat(rt, ZS_FRAME as i64, a, ZValue::int(1), &mut c), ZS_E_TYPE);

        let mut arr = ZValue::NULL;
        assert_eq!(zs_array_new(rt, ZS_FRAME as i64, 2, &mut arr), ZS_OK);
        for i in 0..1000 { assert_eq!(zs_array_push(rt, ZS_FRAME as i64, arr, ZValue::int(i)), ZS_OK); } // wiele wzrostów
        assert_eq!(zs_array_len(arr), 1000);
        let mut v = ZValue::NULL;
        assert_eq!(zs_array_get(arr, 999, &mut v), ZS_OK); assert_eq!(v.as_int(), Some(999));
        assert_eq!(zs_array_get(arr, 1000, &mut v), ZS_E_BOUNDS);
        assert_eq!(zs_array_get(a, 0, &mut v), ZS_E_TYPE);
        assert_eq!(zs_frame_pop(rt), ZS_OK);
        assert_eq!(zs_frame_pop(rt), ZS_E_SCOPE);
        assert_eq!(zs_str_new(rt, ZS_FRAME as i64, b"x".as_ptr(), 1, &mut a), ZS_E_SCOPE, "bez ramki");
        zs_runtime_free(rt);
    }
}

#[test]
fn resources_close_deterministically_and_block_scope_when_retained() {
    unsafe {
        CLOSED.store(0, Ordering::SeqCst);
        let rt = zs_runtime_new(0, 0);
        zs_frame_push(rt);
        let mut h = 0u64;
        assert_eq!(zs_resource_new(rt, ZS_FRAME as i64, std::ptr::null_mut(), Some(close_cb), &mut h), ZS_OK);
        assert_eq!(zs_retain(rt, h), ZS_OK);
        assert_eq!(zs_frame_pop(rt), ZS_E_REFERENCED);
        assert_eq!(CLOSED.load(Ordering::SeqCst), 0);
        zs_release(rt, h);
        assert_eq!(zs_frame_pop(rt), ZS_OK);
        assert_eq!(CLOSED.load(Ordering::SeqCst), 1);
        assert_eq!(zs_handle_live(rt, h), 0);
        // trwały: ostatni release zamyka
        assert_eq!(zs_resource_new(rt, ZS_PERSISTENT as i64, std::ptr::null_mut(), Some(close_cb), &mut h), ZS_OK);
        assert_eq!(zs_release(rt, h), 1);
        assert_eq!(CLOSED.load(Ordering::SeqCst), 2);
        // zamknięcie runtime zamyka pozostałe
        assert_eq!(zs_resource_new(rt, ZS_PERSISTENT as i64, std::ptr::null_mut(), Some(close_cb), &mut h), ZS_OK);
        zs_runtime_free(rt);
        assert_eq!(CLOSED.load(Ordering::SeqCst), 3);
    }
}

#[test]
fn tasks_and_stats() {
    unsafe {
        let rt = zs_runtime_new(0, 1);
        let t = zs_task_new(rt);
        let mut s = ZValue::NULL;
        assert_eq!(zs_str_new(rt, t, b"task".as_ptr(), 4, &mut s), ZS_OK);
        let big = vec![7u8; 2048]; // >= 1 KiB -> LOS (large-kib=1)
        assert_eq!(zs_str_new(rt, t, big.as_ptr(), big.len(), &mut s), ZS_OK);
        let mut st = ZMemStats::default();
        zs_mem_stats(rt, &mut st);
        assert_eq!(st.large_live_blocks, 1); assert!(st.task_used > 0);
        assert_eq!(zs_task_end(rt, t), ZS_OK);
        zs_mem_stats(rt, &mut st);
        assert_eq!((st.large_live_blocks, st.live_handles), (0, 0));
        assert_eq!(zs_str_new(rt, t, b"x".as_ptr(), 1, &mut s), ZS_E_SCOPE, "zadanie zakończone");
        zs_runtime_free(rt);
    }
}
