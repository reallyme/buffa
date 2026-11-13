//! `bytes_type_custom("crate::types::SmallBytes")`

use buffa::{DecodeError, ProtoBytes, WirePayload};

/// A `ProtoBytes` backed by an inline-capable buffer: payloads up to 24 bytes
/// live on the stack, longer ones spill to the heap.
///
/// Generated JSON delegates to this type's native serde in every field shape.
/// Forwarding to buffa's bytes helper preserves protobuf base64 semantics.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
#[repr(transparent)]
pub struct SmallBytes(pub smallvec::SmallVec<[u8; 24]>);
super::assert_transparent!(SmallBytes, smallvec::SmallVec<[u8; 24]>);

impl core::ops::Deref for SmallBytes {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
impl AsRef<[u8]> for SmallBytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl From<Vec<u8>> for SmallBytes {
    #[inline]
    fn from(v: Vec<u8>) -> Self {
        Self(smallvec::SmallVec::from_vec(v))
    }
}
impl ProtoBytes for SmallBytes {
    #[inline]
    fn from_wire(payload: WirePayload<'_>) -> Result<Self, DecodeError> {
        Ok(Self(smallvec::SmallVec::from_slice(payload.as_slice())))
    }
}

impl serde::Serialize for SmallBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        buffa::json_helpers::bytes::serialize(self.as_ref(), serializer)
    }
}
impl<'de> serde::Deserialize<'de> for SmallBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        buffa::json_helpers::bytes::deserialize(deserializer)
    }
}
