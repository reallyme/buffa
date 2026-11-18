//! Cleanup runs on valid live values before their representation is overwritten.

#![cfg(feature = "zeroize")]

use buffa::sensitive::zeroize_vec;
use zeroize::Zeroize;

#[test]
fn niche_values_remain_valid_until_their_destructor_finishes() {
    use std::num::NonZeroU64;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static DROPPED: AtomicUsize = AtomicUsize::new(0);

    #[repr(transparent)]
    struct Niche(NonZeroU64);
    impl Zeroize for Niche {
        fn zeroize(&mut self) {
            self.0 = NonZeroU64::new(7).unwrap();
        }
    }
    impl Drop for Niche {
        fn drop(&mut self) {
            assert_eq!(self.0.get(), 7);
            DROPPED.fetch_add(1, Ordering::SeqCst);
            self.0 = NonZeroU64::new(0x3737_3737_3737_3737).unwrap();
        }
    }

    let mut values = Vec::with_capacity(3);
    values.push(Niche(NonZeroU64::new(19).unwrap()));
    let pointer = values.as_ptr().cast::<u64>();
    zeroize_vec(&mut values);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 1);
    assert!(values.is_empty());
    // SAFETY: this padding-free slot is fully initialized even if the final
    // wipe regresses, because Drop writes its entire u64 representation. The
    // vector still owns the aligned allocation. No invalid Niche is read.
    assert_eq!(unsafe { pointer.read() }, 0);
}

#[test]
fn indirect_owners_are_cleaned_before_being_dropped() {
    use std::cell::Cell;
    use std::rc::Rc;

    struct Owner {
        bytes: Vec<u8>,
        cleaned: Rc<Cell<bool>>,
        dropped: Rc<Cell<bool>>,
    }
    impl Zeroize for Owner {
        fn zeroize(&mut self) {
            self.bytes.zeroize();
            self.cleaned.set(true);
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            assert!(self.cleaned.get());
            assert!(self.bytes.is_empty());
            self.dropped.set(true);
        }
    }

    let cleaned = Rc::new(Cell::new(false));
    let dropped = Rc::new(Cell::new(false));
    let mut values = vec![Owner {
        bytes: vec![0x5a; 19],
        cleaned: Rc::clone(&cleaned),
        dropped: Rc::clone(&dropped),
    }];
    zeroize_vec(&mut values);
    assert!(cleaned.get());
    assert!(dropped.get());
    assert!(values.is_empty());
}

#[test]
fn zero_sized_elements_are_cleaned_and_dropped_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CLEANED: AtomicUsize = AtomicUsize::new(0);
    static DROPPED: AtomicUsize = AtomicUsize::new(0);

    struct Empty;
    impl Zeroize for Empty {
        fn zeroize(&mut self) {
            CLEANED.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Drop for Empty {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::SeqCst);
        }
    }
    let mut values = vec![Empty, Empty, Empty];
    zeroize_vec(&mut values);
    assert!(values.is_empty());
    assert_eq!(CLEANED.load(Ordering::SeqCst), 3);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 3);
    zeroize_vec(&mut values);
    drop(values);
    assert_eq!(CLEANED.load(Ordering::SeqCst), 3);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 3);
}
