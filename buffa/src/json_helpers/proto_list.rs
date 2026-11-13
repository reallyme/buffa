// Modified by ReallyMe Technologies Limited.
//! Protobuf JSON sequences decoded directly into a custom repeated collection.
//!
//! Unlike `proto_seq`, this path constructs the destination before reading any
//! elements. Its allocation and destruction policy therefore also governs
//! growth and cleanup when a later element fails to decode.

use super::{ProtoElemJson, ProtoElemSeed};
use crate::ProtoList;
use core::marker::PhantomData;
use serde::de::{SeqAccess, Visitor};
use serde::Deserializer;

pub use super::proto_seq::serialize;

/// Deserialize a protobuf JSON array, or `null` as an empty collection.
///
/// Each element follows [`ProtoElemJson`], including rejection of null elements.
/// No intermediate `Vec` is allocated and no capacity hint forces the custom
/// collection to allocate before its own insertion policy chooses to do so.
///
/// # Errors
///
/// Returns the deserializer's error for invalid elements or input shape. The
/// partially populated collection is dropped before returning an error.
pub fn deserialize<'de, T, C, D>(deserializer: D) -> Result<C, D::Error>
where
    T: ProtoElemJson,
    C: ProtoList<T>,
    D: Deserializer<'de>,
{
    struct Elements<T, C>(PhantomData<fn() -> (T, C)>);

    impl<'de, T: ProtoElemJson, C: ProtoList<T>> Visitor<'de> for Elements<T, C> {
        type Value = C;

        fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("an array or null")
        }

        fn visit_unit<E>(self) -> Result<C, E> {
            Ok(C::default())
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<C, A::Error> {
            let mut collection = C::default();
            while let Some(element) = sequence.next_element_seed(ProtoElemSeed::<T>(PhantomData))? {
                ProtoList::push(&mut collection, element);
            }
            Ok(collection)
        }
    }

    deserializer.deserialize_any(Elements::<T, C>(PhantomData))
}

#[cfg(test)]
mod tests;
