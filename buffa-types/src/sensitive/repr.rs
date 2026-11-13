//! Wipe-owning representations for the guarded WKT family.
//! Clone is required by the runtime; every copy remains a separate wipe owner.

use alloc::{borrow::ToOwned, boxed::Box, string::String, vec::Vec};
use buffa::bytes::Buf;
#[cfg(feature = "json")]
use buffa::json_helpers::ProtoStringJson;
use buffa::FieldPayload;
use buffa::{DecodeError, ProtoString, WirePayload};
use core::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};
use secrecy::{ExposeSecret, SecretBox};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Copy one bounded field payload into a single wipe-owned allocation.
fn read_guarded(payload: &mut FieldPayload<impl Buf>) -> Result<Zeroizing<Vec<u8>>, DecodeError> {
    let mut bytes = Zeroizing::new(Vec::new());
    // The length is already checked against the input, and the reservation
    // happens before any secret byte is written, so no reallocation can leave
    // a copy behind.
    bytes
        .try_reserve_exact(payload.remaining())
        .map_err(|_| DecodeError::MessageTooLarge)?;
    while payload.has_remaining() {
        let chunk = payload.chunk();
        bytes.extend_from_slice(chunk);
        let count = chunk.len();
        payload.advance(count);
    }
    Ok(bytes)
}

/// UTF-8 storage whose live bytes and spare capacity are wiped on drop.
pub struct SecretString(SecretBox<String>);
impl Clone for SecretString {
    fn clone(&self) -> Self {
        self.0.expose_secret().clone().into()
    }
}
impl Default for SecretString {
    fn default() -> Self {
        String::new().into()
    }
}
impl From<String> for SecretString {
    fn from(value: String) -> Self {
        // Preserve the String allocation and its spare capacity under the
        // wipe owner rather than shrinking it into a boxed str.
        Self(SecretBox::new(Box::new(value)))
    }
}
impl Deref for SecretString {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.expose_secret()
    }
}
impl AsRef<str> for SecretString {
    fn as_ref(&self) -> &str {
        self
    }
}
impl core::borrow::Borrow<str> for SecretString {
    fn borrow(&self) -> &str {
        self
    }
}
impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString([REDACTED])")
    }
}
impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        bool::from(self.as_bytes().ct_eq(other.as_bytes()))
    }
}
impl Eq for SecretString {}
impl PartialOrd for SecretString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SecretString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}
impl Hash for SecretString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}
impl Zeroize for SecretString {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl ZeroizeOnDrop for SecretString {}
impl ProtoString for SecretString {
    fn from_wire(payload: WirePayload<'_>) -> Result<Self, DecodeError> {
        payload.to_str().map(|value| value.to_owned().into())
    }
    fn decode_length_delimited(payload: &mut FieldPayload<impl Buf>) -> Result<Self, DecodeError> {
        let mut bytes = read_guarded(payload)?;
        if core::str::from_utf8(&bytes).is_err() {
            return Err(DecodeError::InvalidUtf8);
        }
        // Validated above, so the conversion keeps the guarded allocation. A
        // failure would hand the bytes back, and they are wiped before return.
        match String::from_utf8(core::mem::take(&mut *bytes)) {
            Ok(text) => Ok(text.into()),
            Err(error) => {
                let mut rejected = error.into_bytes();
                rejected.zeroize();
                Err(DecodeError::InvalidUtf8)
            }
        }
    }
}
#[cfg(feature = "json")]
impl ProtoStringJson for SecretString {
    fn serialize_json<S: serde::Serializer>(text: &str, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(text)
    }

    fn deserialize_json<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        struct TextVisitor;
        impl serde::de::Visitor<'_> for TextVisitor {
            type Value = SecretString;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(value.to_owned().into())
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(value.into())
            }
        }
        deserializer.deserialize_string(TextVisitor)
    }
}

#[cfg(feature = "json")]
macro_rules! serde_policy {
    ($name:ident,$policy:ident) => {
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                <Self as $policy>::serialize_json(self.as_ref(), serializer)
            }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                <Self as $policy>::deserialize_json(deserializer)
            }
        }
    };
}
#[cfg(feature = "json")]
serde_policy!(SecretString, ProtoStringJson);

#[cfg(feature = "json")]
macro_rules! element_policy {
    ($name:ident, $policy:ident) => {
        impl buffa::json_helpers::ProtoElemJson for $name {
            fn serialize_proto_json<S: serde::Serializer>(
                value: &Self,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                <Self as $policy>::serialize_json(value.as_ref(), serializer)
            }
            fn deserialize_proto_json<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                <Self as $policy>::deserialize_json(deserializer)
            }
        }
    };
}
#[cfg(feature = "json")]
element_policy!(SecretString, ProtoStringJson);

#[cfg(feature = "reflect")]
impl buffa_descriptor::reflect::InspectElement for SecretString {
    fn as_inspect_value(&self) -> buffa_descriptor::reflect::InspectValueRef<'_> {
        buffa_descriptor::reflect::InspectValueRef::String(self)
    }
}
#[cfg(feature = "reflect")]
impl buffa_descriptor::reflect::ReflectElement for SecretString {
    fn as_value_ref(&self) -> buffa_descriptor::reflect::ValueRef<'_> {
        buffa_descriptor::reflect::ValueRef::String(self)
    }
}
#[cfg(feature = "reflect")]
impl buffa_descriptor::reflect::ReflectMapKey for SecretString {
    fn as_map_key_ref(&self) -> buffa_descriptor::reflect::MapKeyRef<'_> {
        buffa_descriptor::reflect::MapKeyRef::String(self)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}
