// Modified by ReallyMe Technologies Limited.
//! View containers that own cleanup, without changing ordinary view drop checking.
use crate::sensitive::{view_storage, Zeroize, ZeroizeOnDrop};
use alloc::vec::Vec;
use core::ops::Deref;

/// A decoded repeated view that wipes live and outgrown collection storage.
/// Borrowed elements retain their caller-owned input lifetime.
#[derive(Clone, PartialEq, Eq)]
pub struct RepeatedView<'a, T>(super::RepeatedView<'a, T>);
impl<'a, T> RepeatedView<'a, T> {
    /// Adopt an existing allocation without shrinking it.
    pub fn new(values: Vec<T>) -> Self {
        Self(super::RepeatedView::new(values))
    }
    /// Append while wiping an allocation before replacing it.
    pub fn push(&mut self, value: T) {
        view_storage::reserve(&mut self.0.elements, 1);
        self.0.elements.push(value);
    }
    /// Reserve without releasing an unwiped old buffer.
    pub fn reserve(&mut self, additional: usize) {
        view_storage::reserve(&mut self.0.elements, additional);
    }
    /// Borrow the decoded elements.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.0.iter()
    }
}
impl<T> Default for RepeatedView<'_, T> {
    fn default() -> Self {
        Self(super::RepeatedView::default())
    }
}
impl<T> Deref for RepeatedView<'_, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}
impl<T> Zeroize for RepeatedView<'_, T> {
    fn zeroize(&mut self) {
        view_storage::clear(&mut self.0.elements);
    }
}
impl<T> Drop for RepeatedView<'_, T> {
    fn drop(&mut self) {
        self.zeroize();
    }
}
impl<T> ZeroizeOnDrop for RepeatedView<'_, T> {}
impl<T> core::fmt::Debug for RepeatedView<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RepeatedView([REDACTED])")
    }
}
impl<T> From<Vec<T>> for RepeatedView<'_, T> {
    fn from(values: Vec<T>) -> Self {
        Self::new(values)
    }
}
impl<T> FromIterator<T> for RepeatedView<'_, T> {
    fn from_iter<I: IntoIterator<Item = T>>(source: I) -> Self {
        let mut out = Self::default();
        for value in source {
            out.push(value);
        }
        out
    }
}
impl<T> IntoIterator for RepeatedView<'_, T> {
    type Item = T;
    type IntoIter = crate::sensitive::ViewIntoIter<T>;
    fn into_iter(mut self) -> Self::IntoIter {
        Self::IntoIter::new(core::mem::take(&mut self.0.elements))
    }
}
impl<'b, T> IntoIterator for &'b RepeatedView<'_, T> {
    type Item = &'b T;
    type IntoIter = core::slice::Iter<'b, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A decoded map view that wipes owned entry storage on growth and drop.
/// Lookup and duplicate-key handling match the ordinary map view.
#[derive(Clone, PartialEq, Eq)]
pub struct MapView<'a, K, V>(super::MapView<'a, K, V>);
impl<'a, K, V> MapView<'a, K, V> {
    /// Adopt an existing allocation without shrinking it.
    pub fn new(entries: Vec<(K, V)>) -> Self {
        Self(super::MapView::new(entries))
    }
    /// Append a wire entry, retaining last-occurrence lookup semantics.
    pub fn push(&mut self, key: K, value: V) {
        view_storage::reserve(&mut self.0.entries, 1);
        self.0.entries.push((key, value));
    }
}
impl<K, V> Default for MapView<'_, K, V> {
    fn default() -> Self {
        Self(super::MapView::default())
    }
}
impl<'a, K, V> Deref for MapView<'a, K, V> {
    type Target = super::MapView<'a, K, V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<K, V> Zeroize for MapView<'_, K, V> {
    fn zeroize(&mut self) {
        view_storage::clear(&mut self.0.entries);
    }
}
impl<K, V> Drop for MapView<'_, K, V> {
    fn drop(&mut self) {
        self.zeroize();
    }
}
impl<K, V> ZeroizeOnDrop for MapView<'_, K, V> {}
impl<K, V> core::fmt::Debug for MapView<'_, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MapView([REDACTED])")
    }
}
impl<K, V> From<Vec<(K, V)>> for MapView<'_, K, V> {
    fn from(entries: Vec<(K, V)>) -> Self {
        Self::new(entries)
    }
}
impl<K, V> FromIterator<(K, V)> for MapView<'_, K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(source: I) -> Self {
        let mut out = Self::default();
        for (key, value) in source {
            out.push(key, value);
        }
        out
    }
}
impl<K, V> IntoIterator for MapView<'_, K, V> {
    type Item = (K, V);
    type IntoIter = crate::sensitive::ViewIntoIter<(K, V)>;
    fn into_iter(mut self) -> Self::IntoIter {
        Self::IntoIter::new(core::mem::take(&mut self.0.entries))
    }
}
impl<'b, K, V> IntoIterator for &'b MapView<'_, K, V> {
    type Item = &'b (K, V);
    type IntoIter = core::slice::Iter<'b, (K, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
