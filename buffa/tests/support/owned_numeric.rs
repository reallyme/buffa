//! Transferred numeric text is wiped while its allocation is still owned.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

struct Observer;
static WATCHED: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static LENGTH: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicBool = AtomicBool::new(false);
static WIPED: AtomicBool = AtomicBool::new(false);

// SAFETY: allocation and release are forwarded unchanged. The only inspected
// block is an explicitly registered, initialized byte prefix of a String.
// Inspection happens before System.dealloc, never through a freed pointer.
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
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
            let length = LENGTH.load(Ordering::SeqCst);
            let wiped = if length <= layout.size() {
                // SAFETY: the registered String's observed prefix was filled
                // before registration. Conversion only moves this byte owner.
                unsafe { std::slice::from_raw_parts(pointer, length) }
                    .iter()
                    .all(|byte| *byte == 0)
            } else {
                false
            };
            WIPED.store(wiped, Ordering::SeqCst);
            RELEASED.store(true, Ordering::SeqCst);
        }
        // SAFETY: the caller's allocation and layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Observer = Observer;

pub(super) fn observe(token: &str, expected: bool, decode: impl FnOnce(String) -> bool) {
    // Preserve the initialized prefix length before shortening the token;
    // observing it is sound even if cleanup regresses. No padding is read.
    let mut text = "7".repeat(97);
    let initialized = text.len();
    text.clear();
    text.push_str(token);
    RELEASED.store(false, Ordering::SeqCst);
    WIPED.store(false, Ordering::SeqCst);
    LENGTH.store(initialized, Ordering::SeqCst);
    WATCHED.store(text.as_mut_ptr(), Ordering::SeqCst);
    let accepted = decode(text);
    assert!(
        RELEASED.load(Ordering::SeqCst),
        "owned token was not released"
    );
    assert!(
        WIPED.load(Ordering::SeqCst),
        "owned numeric token was released without wiping"
    );
    assert_eq!(accepted, expected);
}

/// A zero result is meaningful only if the observer sees ordinary releases.
pub(super) fn assert_detects_unwiped_drop() {
    let mut control = "7".repeat(97);
    RELEASED.store(false, Ordering::SeqCst);
    WIPED.store(false, Ordering::SeqCst);
    LENGTH.store(control.len(), Ordering::SeqCst);
    WATCHED.store(control.as_mut_ptr(), Ordering::SeqCst);
    drop(control);
    assert!(RELEASED.load(Ordering::SeqCst));
    assert!(!WIPED.load(Ordering::SeqCst));
}

/// Rejected owned byte tokens obey the same allocation boundary.
pub(super) fn observe_bytes(reject: impl FnOnce(Vec<u8>) -> bool) {
    let mut bytes = vec![0x37; 97];
    LENGTH.store(bytes.len(), Ordering::SeqCst);
    RELEASED.store(false, Ordering::SeqCst);
    WIPED.store(false, Ordering::SeqCst);
    WATCHED.store(bytes.as_mut_ptr(), Ordering::SeqCst);
    assert!(reject(bytes));
    assert!(RELEASED.load(Ordering::SeqCst));
    assert!(WIPED.load(Ordering::SeqCst));
}
