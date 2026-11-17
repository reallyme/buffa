// Modified by ReallyMe Technologies Limited.
//! Temporary map slots; cleanup is opt-in with the `zeroize` feature.
#[cfg(feature = "zeroize")]
pub use guarded::Erasing;
#[cfg(not(feature = "zeroize"))]
pub use plain::Erasing;

// The ordinary codec retains a move-only slot with no destructor or unsafe
// storage access. The optimizer can remove this wrapper entirely.
#[cfg(not(feature = "zeroize"))]
mod plain {
    use core::ops::{Deref, DerefMut};
    pub struct Erasing<T>(T);
    impl<T> Erasing<T> {
        pub fn new(value: T, _erase: fn(&mut T)) -> Self {
            Self(value)
        }
        pub fn into_inner(self) -> T {
            self.0
        }
    }
    impl<T> Deref for Erasing<T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.0
        }
    }
    impl<T> DerefMut for Erasing<T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }
}

#[cfg(feature = "zeroize")]
mod guarded {
    use core::{
        mem::MaybeUninit,
        ops::{Deref, DerefMut},
    };

    /// A partially decoded slot with a fixed cleanup function.
    /// The cleanup function must not panic or allocate.
    #[doc(hidden)]
    pub struct Erasing<T> {
        value: MaybeUninit<T>,
        initialized: bool,
        erase: fn(&mut T),
    }
    impl<T> Erasing<T> {
        pub fn new(value: T, erase: fn(&mut T)) -> Self {
            Self {
                value: MaybeUninit::new(value),
                initialized: true,
                erase,
            }
        }
        pub fn into_inner(mut self) -> T {
            // SAFETY: new initializes the slot. This method consumes the guard,
            // so no other access can observe the now-vacant storage.
            let value = unsafe { self.value.assume_init_read() };
            self.initialized = false;
            value
        }
    }
    impl<T> Deref for Erasing<T> {
        type Target = T;
        fn deref(&self) -> &T {
            // SAFETY: only into_inner marks this false, and it consumes self.
            unsafe { self.value.assume_init_ref() }
        }
    }
    impl<T> DerefMut for Erasing<T> {
        fn deref_mut(&mut self) -> &mut T {
            // SAFETY: same initialized-slot invariant, with an exclusive borrow.
            unsafe { self.value.assume_init_mut() }
        }
    }
    impl<T> Drop for Erasing<T> {
        fn drop(&mut self) {
            if self.initialized {
                // SAFETY: initialized means the value was not transferred. Cleanup
                // runs before its destructor, including on a parse error.
                unsafe {
                    (self.erase)(self.value.assume_init_mut());
                    self.value.assume_init_drop();
                }
            }
            // Erase vacated inline scalar bytes as well as any inactive payload.
            // MaybeUninit permits zero bytes even for types with invalid zero values.
            zeroize::Zeroize::zeroize(&mut self.value);
        }
    }
}

#[cfg(all(test, feature = "zeroize"))]
mod tests {
    use super::*;
    use alloc::{boxed::Box, rc::Rc};
    use core::cell::Cell;
    use core::sync::atomic::{AtomicUsize, Ordering};
    static ERASED: AtomicUsize = AtomicUsize::new(0);
    fn erase(value: &mut u64) {
        assert_eq!(*value, 42);
        *value = 0;
        ERASED.fetch_add(1, Ordering::SeqCst);
    }
    #[test]
    fn cleanup_runs_on_discard_but_not_after_transfer() {
        ERASED.store(0, Ordering::SeqCst);
        drop(Erasing::new(42, erase));
        assert_eq!(ERASED.load(Ordering::SeqCst), 1);
        let value = Erasing::new(42, erase).into_inner();
        assert_eq!(value, 42);
        assert_eq!(ERASED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn heap_ownership_is_transferred_or_destroyed_exactly_once() {
        struct OwnedSlot {
            number: Box<u64>,
            erased: Rc<Cell<usize>>,
            dropped: Rc<Cell<usize>>,
            observed: Rc<Cell<u64>>,
        }
        impl OwnedSlot {
            fn erase(&mut self) {
                *self.number = 0;
                self.erased.set(self.erased.get() + 1);
            }
        }
        impl Drop for OwnedSlot {
            fn drop(&mut self) {
                self.observed.set(*self.number);
                self.dropped.set(self.dropped.get() + 1);
            }
        }

        let erased = Rc::new(Cell::new(0));
        let dropped = Rc::new(Cell::new(0));
        let observed = Rc::new(Cell::new(0));
        let make_slot = || OwnedSlot {
            number: Box::new(37),
            erased: erased.clone(),
            dropped: dropped.clone(),
            observed: observed.clone(),
        };

        // A failed parse destroys its partial owner after cleanup, even when
        // the value owns allocations and therefore has invalid all-zero bits.
        let fail = || -> Result<(), ()> {
            let mut slot = Erasing::new(make_slot(), OwnedSlot::erase);
            *slot.number = 99;
            Err(())
        };
        assert_eq!(fail(), Err(()));
        assert_eq!(erased.get(), 1);
        assert_eq!(dropped.get(), 1);
        assert_eq!(observed.get(), 0);

        // Wiping the vacated slot must not invalidate or destroy the ownership
        // that into_inner moved out. Miri checks the Box and Rc provenance.
        let transferred = Erasing::new(make_slot(), OwnedSlot::erase).into_inner();
        assert_eq!(*transferred.number, 37);
        assert_eq!(erased.get(), 1);
        assert_eq!(dropped.get(), 1);
        drop(transferred);
        assert_eq!(erased.get(), 1);
        assert_eq!(dropped.get(), 2);
        assert_eq!(observed.get(), 37);
    }
}
