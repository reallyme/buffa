// Modified by ReallyMe Technologies Limited.
//! Borrowed reflection without dynamic snapshots or serialization entry points.
//!
//! Guarded messages implement this surface instead of `ReflectMessage`:
//! inspecting a child never encodes it or creates an ordinary dynamic copy.
//! Export remains the generated owned message's JSON policy.

use super::MapKeyRef;
#[cfg(feature = "zeroize")]
use super::ReflectMapKey;
use crate::{DescriptorPool, FieldDescriptor, MessageDescriptor};
use alloc::sync::Arc;
use buffa::sensitive::{Zeroize, ZeroizeOnDrop};
use core::{fmt, ops::Deref};

/// A checked inspection failure, without schema names or message contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InspectErrorReason {
    /// The descriptor is not the exact field from this message's pool.
    #[error("field descriptor does not belong to the inspected message")]
    FieldNotMember,
}

/// Read-only inspection of a guarded message and its declared fields.
///
/// Unlike `ReflectMessage`, this trait has no dynamic-snapshot operation.
/// Returned strings, bytes, children and collections borrow their source.
/// Explicitly copying an inspected value remains the caller's responsibility.
pub trait InspectMessage: Zeroize + ZeroizeOnDrop {
    /// The descriptor owned by this message's embedded pool.
    fn message_descriptor(&self) -> &MessageDescriptor;
    /// The pool containing the descriptor and its referenced types.
    fn pool(&self) -> &Arc<DescriptorPool>;
    /// Read a declared field, preserving protobuf default semantics.
    ///
    /// # Errors
    /// Returns `FieldNotMember` for a foreign or unrelated descriptor, including
    /// a structurally identical descriptor from another pool.
    fn get(&self, field: &FieldDescriptor) -> Result<InspectValueRef<'_>, InspectErrorReason>;
    /// Whether a declared field is present. Foreign descriptors return false.
    fn has(&self, field: &FieldDescriptor) -> bool;
    /// Visit present fields without constructing a dynamic message.
    ///
    /// # Errors
    /// Returns an implementation's checked field-access error.
    fn for_each_set<'a>(
        &'a self,
        visitor: &mut dyn FnMut(&'a FieldDescriptor, InspectValueRef<'a>),
    ) -> Result<(), InspectErrorReason> {
        for field in self.message_descriptor().fields() {
            if self.has(field) {
                visitor(field, self.get(field)?);
            }
        }
        Ok(())
    }
    /// Borrow an inspection handle. It cannot serialize or create a snapshot.
    fn inspect(&self) -> InspectMessageRef<'_>
    where
        Self: Sized,
    {
        InspectMessageRef::Borrowed(self)
    }
}

/// An inspection handle borrowing its message for its entire lifetime.
///
/// It deliberately does not implement `Serialize` or expose `to_dynamic`.
///
/// ```compile_fail
/// use buffa_descriptor::reflect::InspectMessageRef;
/// fn snapshot(value: InspectMessageRef<'_>) {
///     value.to_dynamic();
/// }
/// ```
#[derive(Clone, Copy)]
pub enum InspectMessageRef<'a> {
    /// A live message; no owned or lazily materialized snapshot is stored.
    Borrowed(&'a dyn InspectMessage),
}
impl<'a> InspectMessageRef<'a> {
    /// Borrow a concrete inspected message, including an unset default child.
    pub fn new(message: &'a dyn InspectMessage) -> Self {
        Self::Borrowed(message)
    }
}
impl<'a> Deref for InspectMessageRef<'a> {
    type Target = dyn InspectMessage + 'a;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(message) => *message,
        }
    }
}
impl fmt::Debug for InspectMessageRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InspectMessageRef([REDACTED])")
    }
}

/// A field value borrowed from an inspected message.
///
/// Scalar slots own and wipe their copies on drop. Explicitly extracting a
/// scalar transfers responsibility for that copy to the caller. No owned
/// strings, byte buffers or dynamic message snapshots are materialized.
#[non_exhaustive]
pub enum InspectValueRef<'a> {
    /// A signed 32-bit protobuf scalar.
    I32(i32),
    /// A signed 64-bit protobuf scalar.
    I64(i64),
    /// An unsigned 32-bit protobuf scalar.
    U32(u32),
    /// An unsigned 64-bit protobuf scalar.
    U64(u64),
    /// A 32-bit floating-point scalar.
    F32(f32),
    /// A 64-bit floating-point scalar.
    F64(f64),
    /// A boolean scalar.
    Bool(bool),
    /// Borrowed UTF-8 text under its original owner.
    String(&'a str),
    /// Borrowed bytes under their original owner.
    Bytes(&'a [u8]),
    /// The protobuf numeric enum value.
    EnumNumber(i32),
    /// A nested message with the same borrowed-only inspection contract.
    Message(InspectMessageRef<'a>),
    /// Borrowed repeated elements.
    List(&'a dyn InspectList),
    /// Borrowed map entries.
    Map(&'a dyn InspectMap),
}
impl fmt::Debug for InspectValueRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InspectValueRef([REDACTED])")
    }
}
impl Zeroize for InspectValueRef<'_> {
    fn zeroize(&mut self) {
        match self {
            Self::I32(value) | Self::EnumNumber(value) => value.zeroize(),
            Self::I64(value) => value.zeroize(),
            Self::U32(value) => value.zeroize(),
            Self::U64(value) => value.zeroize(),
            Self::F32(value) => value.zeroize(),
            Self::F64(value) => value.zeroize(),
            Self::Bool(value) => value.zeroize(),
            // Borrowed storage remains under its source's cleanup owner.
            Self::String(_) | Self::Bytes(_) | Self::Message(_) | Self::List(_) | Self::Map(_) => {}
        }
    }
}
impl ZeroizeOnDrop for InspectValueRef<'_> {}
impl Drop for InspectValueRef<'_> {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Borrow one repeated element or map value without making a snapshot.
pub trait InspectElement {
    /// Inspect the same protobuf value kind on every call.
    fn as_inspect_value(&self) -> InspectValueRef<'_>;
}
/// Borrowed repeated-element access. Out-of-range indices return `None`.
pub trait InspectList {
    /// Number of elements.
    fn len(&self) -> usize;
    /// Whether there are no elements.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Borrow an element by index.
    fn get(&self, index: usize) -> Option<InspectValueRef<'_>>;
}
/// Borrowed map access; keys and values remain under their original owners.
pub trait InspectMap {
    /// Number of distinct keys.
    fn len(&self) -> usize;
    /// Whether there are no entries.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Find a key without taking ownership of its numeric or string slot.
    /// Erased lookup can scan O(n) entries. Prefer `for_each` when traversing
    /// a map: it supplies each value directly, avoiding repeated lookups.
    /// A concrete owned `sensitive::Map::get` retains logarithmic lookup.
    fn get(&self, key: &MapKeyRef<'_>) -> Option<InspectValueRef<'_>>;
    /// Visit each logical entry once, with last-occurrence semantics for views.
    /// Borrowed keys and values may be retained for the map's borrow lifetime.
    fn for_each<'a>(&'a self, visitor: &mut dyn FnMut(InspectMapKeyRef<'a>, InspectValueRef<'a>));
}

/// An inspected map key whose copied numeric slot is wiped on drop.
/// String keys continue borrowing their original storage.
pub struct InspectMapKeyRef<'a>(MapKeyRef<'a>);
impl<'a> InspectMapKeyRef<'a> {
    /// Borrow the key's protobuf kind and value without copying its scalar slot.
    pub fn value(&self) -> &MapKeyRef<'a> {
        &self.0
    }
}
impl<'a> From<MapKeyRef<'a>> for InspectMapKeyRef<'a> {
    fn from(value: MapKeyRef<'a>) -> Self {
        Self(value)
    }
}
impl Zeroize for InspectMapKeyRef<'_> {
    fn zeroize(&mut self) {
        match &mut self.0 {
            MapKeyRef::I32(value) => value.zeroize(),
            MapKeyRef::I64(value) => value.zeroize(),
            MapKeyRef::U32(value) => value.zeroize(),
            MapKeyRef::U64(value) => value.zeroize(),
            MapKeyRef::Bool(value) => value.zeroize(),
            MapKeyRef::String(_) => {}
        }
    }
}
impl ZeroizeOnDrop for InspectMapKeyRef<'_> {}
impl Drop for InspectMapKeyRef<'_> {
    fn drop(&mut self) {
        self.zeroize();
    }
}
impl fmt::Debug for InspectMapKeyRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InspectMapKeyRef([REDACTED])")
    }
}

macro_rules! scalar {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl InspectElement for $ty {
            fn as_inspect_value(&self) -> InspectValueRef<'_> {
                InspectValueRef::$variant(*self)
            }
        }
    )*};
}
scalar! { i32 => I32, i64 => I64, u32 => U32, u64 => U64,
f32 => F32, f64 => F64, bool => Bool }
impl InspectElement for &str {
    fn as_inspect_value(&self) -> InspectValueRef<'_> {
        InspectValueRef::String(self)
    }
}
impl InspectElement for &[u8] {
    fn as_inspect_value(&self) -> InspectValueRef<'_> {
        InspectValueRef::Bytes(self)
    }
}
impl<E: buffa::Enumeration> InspectElement for buffa::EnumValue<E> {
    fn as_inspect_value(&self) -> InspectValueRef<'_> {
        InspectValueRef::EnumNumber(self.to_i32())
    }
}

macro_rules! list {
    ($ty:ty $(, $bound:path)?) => {
        impl<T: InspectElement $(+ $bound)?> InspectList for $ty {
            fn len(&self) -> usize { self[..].len() }
            fn get(&self, index: usize) -> Option<InspectValueRef<'_>> {
                self[..].get(index).map(InspectElement::as_inspect_value)
            }
        }
    };
}
list!(alloc::vec::Vec<T>);
list!(buffa::RepeatedView<'_, T>);
#[cfg(feature = "zeroize")]
list!(buffa::sensitive::List<T>, buffa::sensitive::Zeroize);
#[cfg(feature = "zeroize")]
list!(buffa::sensitive::RepeatedView<'_, T>);

#[cfg(feature = "zeroize")]
impl<K, V> InspectMap for buffa::sensitive::Map<K, V>
where
    K: ReflectMapKey + buffa::sensitive::Zeroize,
    V: InspectElement + buffa::sensitive::Zeroize,
{
    fn len(&self) -> usize {
        self.len()
    }
    fn get(&self, key: &MapKeyRef<'_>) -> Option<InspectValueRef<'_>> {
        self.iter()
            .find(|(k, _)| {
                let candidate = InspectMapKeyRef::from(k.as_map_key_ref());
                candidate.value() == key
            })
            .map(|(_, value)| value.as_inspect_value())
    }
    fn for_each<'a>(&'a self, visitor: &mut dyn FnMut(InspectMapKeyRef<'a>, InspectValueRef<'a>)) {
        for (key, value) in self.iter() {
            visitor(key.as_map_key_ref().into(), value.as_inspect_value());
        }
    }
}
#[cfg(feature = "zeroize")]
impl<K: ReflectMapKey + Ord, V: InspectElement> InspectMap for buffa::sensitive::MapView<'_, K, V> {
    fn len(&self) -> usize {
        self.len_unique()
    }
    fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
    fn get(&self, key: &MapKeyRef<'_>) -> Option<InspectValueRef<'_>> {
        self.iter()
            .rev()
            .find(|(k, _)| {
                let candidate = InspectMapKeyRef::from(k.as_map_key_ref());
                candidate.value() == key
            })
            .map(|(_, value)| value.as_inspect_value())
    }
    fn for_each<'a>(&'a self, visitor: &mut dyn FnMut(InspectMapKeyRef<'a>, InspectValueRef<'a>)) {
        for (key, value) in self.iter_unique() {
            visitor(key.as_map_key_ref().into(), value.as_inspect_value());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_slots_erase_copies_and_leave_borrowed_storage_untouched() {
        let bytes = [37u8, 99];
        let mut slots = [
            InspectValueRef::I32(-37),
            InspectValueRef::I64(-99),
            InspectValueRef::U32(37),
            InspectValueRef::U64(99),
            InspectValueRef::F32(f32::NAN),
            InspectValueRef::F64(-1.25),
            InspectValueRef::Bool(true),
            InspectValueRef::EnumNumber(37),
            InspectValueRef::String("private"),
            InspectValueRef::Bytes(&bytes),
        ];
        for slot in &mut slots {
            slot.zeroize();
        }
        assert!(matches!(slots[0], InspectValueRef::I32(0)));
        assert!(matches!(slots[1], InspectValueRef::I64(0)));
        assert!(matches!(slots[2], InspectValueRef::U32(0)));
        assert!(matches!(slots[3], InspectValueRef::U64(0)));
        assert!(matches!(slots[4], InspectValueRef::F32(0.0)));
        assert!(matches!(slots[5], InspectValueRef::F64(0.0)));
        assert!(matches!(slots[6], InspectValueRef::Bool(false)));
        assert!(matches!(slots[7], InspectValueRef::EnumNumber(0)));
        assert_eq!(bytes, [37, 99]);
        assert!(!alloc::format!("{:?}", slots[8]).contains("private"));
        let mut key = InspectMapKeyRef::from(MapKeyRef::U64(37));
        key.zeroize();
        assert_eq!(key.value(), &MapKeyRef::U64(0));
    }

    #[test]
    fn maps_and_lists_borrow_entries_and_views_keep_last_occurrence_semantics() {
        let values: buffa::sensitive::List<u64> = [37, 99].into_iter().collect();
        let list: &dyn InspectList = &values;
        assert_eq!(list.len(), 2);
        assert!(matches!(list.get(1), Some(InspectValueRef::U64(99))));
        assert!(list.get(2).is_none());
        let mut map = buffa::sensitive::Map::<u64, u64>::default();
        map.insert(1, 37);
        let map: &dyn InspectMap = &map;
        assert!(matches!(
            map.get(&MapKeyRef::U64(1)),
            Some(InspectValueRef::U64(37))
        ));
        assert!(map.get(&MapKeyRef::U64(2)).is_none());
        let view = buffa::sensitive::MapView::new(alloc::vec![(1u64, 37u64), (1, 99)]);
        let map: &dyn InspectMap = &view;
        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());
        assert!(matches!(
            map.get(&MapKeyRef::U64(1)),
            Some(InspectValueRef::U64(99))
        ));
        let mut visited = 0;
        map.for_each(&mut |key, value| {
            assert_eq!(key.value(), &MapKeyRef::U64(1));
            assert!(matches!(
                map.get(key.value()),
                Some(InspectValueRef::U64(99))
            ));
            assert!(matches!(value, InspectValueRef::U64(99)));
            visited += 1;
        });
        assert_eq!(visited, 1);
    }

    #[test]
    fn callbacks_allow_retaining_borrowed_text_without_copying_it() {
        let key = alloc::string::String::from("key");
        let text = alloc::string::String::from("value");
        let view = buffa::sensitive::MapView::new(alloc::vec![(key.as_str(), text.as_str())]);
        let map: &dyn InspectMap = &view;
        let mut keys = alloc::vec::Vec::new();
        let mut values = alloc::vec::Vec::new();
        map.for_each(&mut |key, value| {
            if let MapKeyRef::String(key) = key.value() {
                keys.push(*key);
            }
            if let InspectValueRef::String(value) = value {
                values.push(value);
            }
        });
        assert_eq!(keys, [key.as_str()]);
        assert_eq!(values, [text.as_str()]);
        assert_eq!(keys[0].as_ptr(), key.as_ptr());
        assert_eq!(values[0].as_ptr(), text.as_ptr());
    }
}
