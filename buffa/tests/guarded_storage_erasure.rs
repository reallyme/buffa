//! Observe initialized storage before release, including lazy-view spare slots.
#![cfg(feature = "zeroize")]

use buffa::sensitive::{List, Zeroize};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

struct Observer;
std::thread_local! {
    static LAST: std::cell::Cell<(*mut u8, usize)> = const {
        std::cell::Cell::new((std::ptr::null_mut(), 0))
    };
}
static WATCHED: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static OFFSET: AtomicUsize = AtomicUsize::new(0);
static LENGTH: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicBool = AtomicBool::new(false);
static WIPED: AtomicBool = AtomicBool::new(false);

// SAFETY: every operation forwards the original allocator arguments. The
// observer reads only an explicitly initialized range before System releases
// it: byte-array values have no padding; reference-vector observations cover
// only spare storage filled by this test, never live reference representations.
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplies the valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        // A const thread-local Cell needs no allocation or destructor. Keeping
        // this registration thread-local avoids racing test-harness allocations.
        let _ = LAST.try_with(|last| last.set((pointer, layout.size())));
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WATCHED
            .compare_exchange(
                pointer,
                std::ptr::null_mut(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            let offset = OFFSET.load(Ordering::SeqCst);
            let length = LENGTH.load(Ordering::SeqCst);
            let wiped = offset
                .checked_add(length)
                .is_some_and(|end| end <= layout.size())
                && {
                    // SAFETY: registration covers only initialized bytes within
                    // this allocation; it is still owned and has not been freed.
                    unsafe { std::slice::from_raw_parts(pointer.add(offset), length) }
                        .iter()
                        .all(|byte| *byte == 0)
                };
            WIPED.store(wiped, Ordering::SeqCst);
            RELEASED.store(true, Ordering::SeqCst);
        }
        // SAFETY: the original pointer and layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Observer = Observer;
fn watch(pointer: *mut u8, offset: usize, length: usize) {
    assert_ne!(length, 0);
    OFFSET.store(offset, Ordering::SeqCst);
    LENGTH.store(length, Ordering::SeqCst);
    RELEASED.store(false, Ordering::SeqCst);
    WIPED.store(false, Ordering::SeqCst);
    WATCHED.store(pointer, Ordering::SeqCst);
}
fn released(wiped: bool) {
    assert!(
        RELEASED.load(Ordering::SeqCst),
        "watched allocation was not released"
    );
    assert_eq!(
        WIPED.load(Ordering::SeqCst),
        wiped,
        "released initialized storage was not wiped"
    );
}

/// Poison only spare storage, using the allocator's original full-allocation
/// pointer. A slice pointer alone grants no mutable access to the live entries.
fn watch_last_spare(pointer: *const u8, used: usize) {
    let (allocation, size) = LAST.with(std::cell::Cell::get);
    assert_eq!(allocation.cast_const(), pointer);
    let spare = size.checked_sub(used).unwrap();
    assert_ne!(spare, 0, "fixture needs a vector with spare capacity");
    // SAFETY: the allocator recorded the exact live block and layout. The
    // initialized reference prefix is untouched. Spare capacity is untyped
    // storage; writing bytes there creates no invalid initialized reference.
    unsafe { std::ptr::write_bytes(allocation.add(used), 0x37, spare) };
    watch(allocation, used, spare);
}

#[test]
fn growth_iteration_and_lazy_cleanup_wipe_the_storage_they_release() {
    // Prove the observer detects an ordinary, unwiped byte-array release.
    let mut control = vec![[0x37u8; 8]; 4];
    watch(
        control.as_mut_ptr().cast(),
        0,
        control.len().checked_mul(8).unwrap(),
    );
    drop(control);
    released(false);

    let mut values = List::<[u8; 8]>::default();
    values.push([0x37; 8]);
    while values.len() < values.capacity() {
        values.push([0x37; 8]);
    }
    watch(
        values.as_ptr().cast_mut().cast(),
        0,
        values.len().checked_mul(8).unwrap(),
    );
    values.push([0x37; 8]);
    released(true);

    let pointer = values.as_ptr().cast_mut().cast();
    let length = values.len().checked_mul(8).unwrap();
    let mut iter = values.into_iter();
    assert_eq!(iter.next(), Some([0x37; 8]));
    watch(pointer, 0, length);
    drop(iter);
    released(true);

    let bytes = [0x57u8; 5];
    let allowance = std::cell::Cell::new(1024);
    let ctx = buffa::DecodeContext::new(10, &allowance);
    let mut repeated = buffa::LazyRepeatedView::<()>::new();
    repeated.push_bytes(&bytes, ctx);
    watch_last_spare(
        repeated.raw_elements().as_ptr().cast(),
        std::mem::size_of::<&[u8]>(),
    );
    repeated.zeroize();
    released(true);
    assert!(repeated.is_empty());

    let mut singular = buffa::LazyMessageFieldView::<()>::unset();
    for _ in 0..3 {
        singular.push_fragment(&bytes, ctx);
    }
    watch_last_spare(
        singular.fragments().as_ptr().cast(),
        3usize.checked_mul(std::mem::size_of::<&[u8]>()).unwrap(),
    );
    singular.zeroize();
    released(true);
    assert!(singular.is_unset());
    assert_eq!(bytes, [0x57; 5]);
}
