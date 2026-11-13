use super::{Zeroize, ZeroizeOnDrop};
use alloc::vec::Vec;
use core::{fmt, ops::Deref};

/// A repeated collection that wipes live elements, spare capacity and outgrown buffers.
///
/// Mutable access is limited to individual elements: mutable slice methods can
/// allocate temporary copies outside this collection's cleanup owner.
/// Advisory packed-decode capacity hints are ignored to avoid preallocating
/// from an attacker-influenced upper bound; growth still wipes prior buffers.
/// Growth follows the infallible allocation contract of `ProtoList`: capacity
/// overflow or reservation failure invokes the global allocation-failure handler.
///
/// ```compile_fail
/// let mut values: buffa::sensitive::List<u64> = [3, 1, 2].into_iter().collect();
/// values.sort();
/// ```
///
/// ```compile_fail
/// let mut values: buffa::sensitive::List<u64> = [3, 1, 2].into_iter().collect();
/// values.iter_mut().into_slice().sort();
/// ```
pub struct List<T: Zeroize>(Vec<T>);
impl<T: Zeroize> List<T> {
    /// Append an element without releasing an unwiped old allocation.
    pub fn push(&mut self, value: T) {
        if self.0.len() == self.0.capacity() {
            // This check also covers zero-sized types: their capacity is
            // usize::MAX, and Vec::push would otherwise panic at that length.
            let needed = match self.0.len().checked_add(1) {
                Some(needed) => needed,
                None => alloc::alloc::handle_alloc_error(alloc::alloc::Layout::new::<T>()),
            };
            let capacity = self
                .0
                .capacity()
                .checked_mul(2)
                .unwrap_or(needed)
                .max(needed);
            let mut next = Vec::new();
            // ProtoList's insertion contract is infallible. Reserve before
            // copying any secrets and follow the global allocation-failure
            // handler if the request cannot be satisfied.
            if next.try_reserve_exact(capacity).is_err() {
                alloc::alloc::handle_alloc_error(alloc::alloc::Layout::new::<T>());
            }
            next.append(&mut self.0);
            let mut previous = core::mem::replace(&mut self.0, next);
            // The elements were moved, so only their vacated storage remains.
            previous.zeroize();
        }
        self.0.push(value);
    }
    /// The element capacity currently owned by the collection.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    /// Mutably borrow one element without exposing a mutable slice.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.0.get_mut(index)
    }
    /// Mutably borrow elements without exposing slice allocation methods.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut(self.0.iter_mut())
    }
    /// Erase all elements and spare capacity, leaving the list empty.
    pub fn clear(&mut self) {
        self.0.zeroize();
    }
}
impl<T: Zeroize> Default for List<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<T: Zeroize> Zeroize for List<T> {
    fn zeroize(&mut self) {
        self.clear();
    }
}
impl<T: Zeroize> Drop for List<T> {
    fn drop(&mut self) {
        self.zeroize();
    }
}
impl<T: Zeroize> ZeroizeOnDrop for List<T> {}
impl<T: Zeroize + Clone> Clone for List<T> {
    fn clone(&self) -> Self {
        self.iter().cloned().collect()
    }
}
impl<T: Zeroize + PartialEq> PartialEq for List<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: Zeroize + Eq> Eq for List<T> {}
impl<T: Zeroize> fmt::Debug for List<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("List([REDACTED])")
    }
}
impl<T: Zeroize> Deref for List<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}
impl<T: Zeroize> From<Vec<T>> for List<T> {
    fn from(value: Vec<T>) -> Self {
        Self(value)
    }
}
impl<T: Zeroize> FromIterator<T> for List<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut list = Self::default();
        for value in iter {
            list.push(value);
        }
        list
    }
}
impl<T: Zeroize + Clone + PartialEq + fmt::Debug + Send + Sync> crate::ProtoList<T> for List<T> {
    fn push(&mut self, value: T) {
        self.push(value);
    }
    fn clear(&mut self) {
        self.clear();
    }
}
#[cfg(feature = "json")]
impl<T: Zeroize + serde::Serialize> serde::Serialize for List<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&*self.0, serializer)
    }
}
#[cfg(feature = "json")]
impl<'de, T: Zeroize + serde::Deserialize<'de>> serde::Deserialize<'de> for List<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V<T>(core::marker::PhantomData<T>);
        impl<'de, T: Zeroize + serde::Deserialize<'de>> serde::de::Visitor<'de> for V<T> {
            type Value = List<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an array")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut list = List::default();
                while let Some(value) = seq.next_element()? {
                    list.push(value);
                }
                Ok(list)
            }
        }
        deserializer.deserialize_seq(V(core::marker::PhantomData))
    }
}

impl<'a, T: Zeroize> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: Zeroize> IntoIterator for &'a mut List<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Mutable element borrows that cannot be converted into a mutable slice.
pub struct IterMut<'a, T>(core::slice::IterMut<'a, T>);
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<T> DoubleEndedIterator for IterMut<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}
impl<T> ExactSizeIterator for IterMut<'_, T> {}
impl<T> core::iter::FusedIterator for IterMut<'_, T> {}
impl<T> fmt::Debug for IterMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IterMut([REDACTED])")
    }
}

impl<T: Zeroize> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = super::ViewIntoIter<T>;
    fn into_iter(mut self) -> Self::IntoIter {
        Self::IntoIter::with_cleanup(core::mem::take(&mut self.0), T::zeroize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growth_clone_and_clear_keep_values_owned() {
        let mut values = List::default();
        for n in 0..100u64 {
            values.push(n);
        }
        assert_eq!(values.len(), 100);
        let mut cloned = values.clone();
        *cloned.get_mut(0).unwrap() = 42;
        assert_eq!(values[0], 0);
        assert_eq!(cloned[0], 42);
        values.clear();
        assert!(values.is_empty());
        values.push(7);
        assert_eq!(&*values, &[7]);
    }

    #[test]
    fn mutable_iteration_keeps_elements_in_their_cleanup_owner() {
        let mut values: List<u64> = [1, 2, 3].into_iter().collect();
        assert!(values.get_mut(3).is_none());
        let mut iter = values.iter_mut();
        assert_eq!(iter.len(), 3);
        *iter.next().unwrap() = 7;
        *iter.next_back().unwrap() = 9;
        assert_eq!(iter.len(), 1);
        assert_eq!(alloc::format!("{iter:?}"), "IterMut([REDACTED])");
        for value in &mut values {
            *value += 1;
        }
        assert_eq!(&*values, &[8, 3, 10]);
    }

    #[test]
    fn unconsumed_elements_are_zeroized_even_without_their_own_drop() {
        use alloc::rc::Rc;
        use core::cell::Cell;
        struct Counted(Rc<Cell<usize>>);
        impl Zeroize for Counted {
            fn zeroize(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let erased = Rc::new(Cell::new(0));
        let mut values = List::default();
        values.push(Counted(erased.clone()));
        values.push(Counted(erased.clone()));
        let mut iter = values.into_iter();
        let mut transferred = iter.next().unwrap();
        assert_eq!(erased.get(), 0);
        drop(iter);
        assert_eq!(erased.get(), 1);
        transferred.zeroize();
        assert_eq!(erased.get(), 2);
    }
    #[cfg(feature = "json")]
    #[test]
    fn native_serde_rejects_later_invalid_elements() {
        let values: List<u64> = serde_json::from_str("[1,2,3]").unwrap();
        assert_eq!(&*values, &[1, 2, 3]);
        assert_eq!(serde_json::to_string(&values).unwrap(), "[1,2,3]");
        assert!(serde_json::from_str::<List<u64>>("[1,2,[]]").is_err());
    }
}
