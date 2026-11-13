// Modified by ReallyMe Technologies Limited.
//! Release view-owned slots only after destructors run and vacated bytes are wiped.
use alloc::vec::Vec;
use core::{
    iter::FusedIterator,
    mem::{ManuallyDrop, MaybeUninit},
};
use zeroize::Zeroize;

pub(crate) fn clear<T>(values: &mut Vec<T>) {
    values.clear();
    values.spare_capacity_mut().zeroize();
}

pub(crate) fn reserve<T>(values: &mut Vec<T>, additional: usize) {
    let needed = match values.len().checked_add(additional) {
        Some(needed) => needed,
        None => alloc::alloc::handle_alloc_error(alloc::alloc::Layout::new::<T>()),
    };
    if needed <= values.capacity() {
        return;
    }
    let capacity = values
        .capacity()
        .checked_mul(2)
        .unwrap_or(needed)
        .max(needed);
    let mut next = Vec::new();
    if next.try_reserve_exact(capacity).is_err() {
        alloc::alloc::handle_alloc_error(alloc::alloc::Layout::new::<T>());
    }
    next.append(values);
    let mut previous = core::mem::replace(values, next);
    previous.spare_capacity_mut().zeroize();
}

/// An owning view iterator that wipes consumed slots and its released buffer.
/// Values transferred to the caller become the caller's cleanup responsibility.
#[doc(hidden)]
pub struct IntoIter<T> {
    slots: Vec<MaybeUninit<T>>,
    front: usize,
    back: usize,
    erase: fn(&mut T),
}
impl<T> IntoIter<T> {
    pub(crate) fn new(values: Vec<T>) -> Self {
        Self::with_cleanup(values, |_| {})
    }
    pub(crate) fn with_cleanup(values: Vec<T>, erase: fn(&mut T)) -> Self {
        let mut values = ManuallyDrop::new(values);
        let length = values.len();
        // SAFETY: MaybeUninit<T> has exactly T's layout. The Vec's allocation
        // is transferred once, with unchanged length/capacity and allocator.
        // Only front..back holds live T values; Drop destroys that range once.
        let slots = unsafe {
            Vec::from_raw_parts(
                values.as_mut_ptr().cast::<MaybeUninit<T>>(),
                length,
                values.capacity(),
            )
        };
        Self {
            slots,
            front: 0,
            back: length,
            erase,
        }
    }
    fn take(&mut self, index: usize) -> Option<T> {
        let slot = self.slots.get_mut(index)?;
        // SAFETY: callers remove exactly one index from the initialized range
        // before reading it. The vacated slot is wiped without dropping T.
        let value = unsafe { slot.assume_init_read() };
        slot.zeroize();
        Some(value)
    }
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front = self.front.checked_add(1)?;
        self.take(index)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.back.checked_sub(self.front) {
            Some(remaining) => (remaining, Some(remaining)),
            None => (0, Some(0)),
        }
    }
}
impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        if self.front == self.back {
            return None;
        }
        self.back = self.back.checked_sub(1)?;
        self.take(self.back)
    }
}
impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> FusedIterator for IntoIter<T> {}
impl<T> Drop for IntoIter<T> {
    fn drop(&mut self) {
        for slot in self.slots.iter_mut().take(self.back).skip(self.front) {
            // SAFETY: only this unconsumed range still owns initialized T's.
            unsafe {
                (self.erase)(slot.assume_init_mut());
                slot.assume_init_drop();
            }
        }
        self.slots.zeroize();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::rc::Rc;
    use core::cell::Cell;
    struct Counted(Rc<Cell<usize>>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[test]
    fn consuming_from_both_ends_drops_each_value_once() {
        let dropped = Rc::new(Cell::new(0));
        let mut iter = IntoIter::new((0..4).map(|_| Counted(dropped.clone())).collect());
        assert_eq!(iter.len(), 4);
        drop(iter.next());
        drop(iter.next_back());
        assert_eq!(dropped.get(), 2);
        assert_eq!(iter.len(), 2);
        drop(iter);
        assert_eq!(dropped.get(), 4);
        assert!(IntoIter::<u64>::new(Vec::new()).next().is_none());
    }
}
