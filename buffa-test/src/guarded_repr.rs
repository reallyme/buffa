//! Test-local representations: policy belongs to the consuming proto crate.
//! Clone is required by the runtime; every copy remains a separate wipe owner.

use buffa::{bytes::Buf, DecodeError, ProtoBytes, ProtoString, WirePayload};
use core::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};
use secrecy::{ExposeSecret, SecretBox};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub struct SecretBytes(SecretBox<Vec<u8>>);

impl Clone for SecretBytes {
    fn clone(&self) -> Self {
        self.0.expose_secret().clone().into()
    }
}

impl Default for SecretBytes {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl From<Vec<u8>> for SecretBytes {
    fn from(value: Vec<u8>) -> Self {
        // Box the Vec header without shrinking its buffer: converting to a
        // boxed slice could release spare capacity containing earlier secrets.
        Self(SecretBox::new(Box::new(value)))
    }
}
impl Deref for SecretBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.0.expose_secret()
    }
}
impl AsRef<[u8]> for SecretBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}
impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}
impl PartialEq for SecretBytes {
    fn eq(&self, other: &Self) -> bool {
        bool::from(self.as_ref().ct_eq(other.as_ref()))
    }
}
impl Zeroize for SecretBytes {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl ZeroizeOnDrop for SecretBytes {}

fn read_guarded(buf: &mut impl Buf) -> Result<Zeroizing<Vec<u8>>, DecodeError> {
    let length = buffa::encoding::decode_varint(buf)?;
    let length = usize::try_from(length).map_err(|_| DecodeError::MessageTooLarge)?;
    if length > buf.remaining() {
        return Err(DecodeError::UnexpectedEof);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    bytes
        .try_reserve_exact(length)
        .map_err(|_| DecodeError::MessageTooLarge)?;
    bytes.resize(length, 0);
    let mut offset = 0usize;
    while offset < length {
        let remaining = length
            .checked_sub(offset)
            .ok_or(DecodeError::MessageTooLarge)?;
        let chunk = buf.chunk();
        let count = remaining.min(chunk.len());
        if count == 0 {
            return Err(DecodeError::UnexpectedEof);
        }
        let end = offset
            .checked_add(count)
            .ok_or(DecodeError::MessageTooLarge)?;
        bytes[offset..end].copy_from_slice(&chunk[..count]);
        buf.advance(count);
        offset = end;
    }
    Ok(bytes)
}

impl ProtoBytes for SecretBytes {
    fn from_wire(payload: WirePayload<'_>) -> Result<Self, DecodeError> {
        Ok(payload.as_slice().to_vec().into())
    }
    fn decode(buf: &mut impl Buf) -> Result<Self, DecodeError> {
        let mut bytes = read_guarded(buf)?;
        Ok(core::mem::take(&mut *bytes).into())
    }
}

impl serde::Serialize for SecretBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use base64::Engine as _;
        let encoded =
            Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(self.as_ref()));
        serializer.serialize_str(&encoded)
    }
}

impl<'de> serde::Deserialize<'de> for SecretBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;
        impl serde::de::Visitor<'_> for BytesVisitor {
            type Value = SecretBytes;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a base64 string or null")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(SecretBytes::default())
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let mut bytes = Zeroizing::new(Vec::new());
                // Base64 output never exceeds the input length. Reserve once,
                // before any secret is written, so reallocation cannot expose it.
                bytes
                    .try_reserve_exact(value.len())
                    .map_err(|_| E::custom(GuardedJsonErrorReason::AllocationFailed))?;
                bytes.resize(value.len(), 0);
                let length = decode_base64_into(value, &mut bytes).map_err(E::custom)?;
                bytes.truncate(length);
                Ok(core::mem::take(&mut *bytes).into())
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                let value = Zeroizing::new(value);
                self.visit_str(&value)
            }
        }
        deserializer.deserialize_any(BytesVisitor)
    }
}

fn decode_base64_into(value: &str, output: &mut [u8]) -> Result<usize, GuardedJsonErrorReason> {
    use base64::alphabet;
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    use base64::Engine as _;
    const CONFIG: GeneralPurposeConfig = GeneralPurposeConfig::new()
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let standard = GeneralPurpose::new(&alphabet::STANDARD, CONFIG);
    let url_safe = GeneralPurpose::new(&alphabet::URL_SAFE, CONFIG);
    standard
        .decode_slice(value, &mut *output)
        .or_else(|_| url_safe.decode_slice(value, output))
        .map_err(|_| GuardedJsonErrorReason::InvalidBase64)
}

#[derive(Debug, thiserror::Error)]
enum GuardedJsonErrorReason {
    #[error("invalid base64 in guarded field")]
    InvalidBase64,
    #[error("guarded JSON buffer allocation failed")]
    AllocationFailed,
}

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
    fn decode(buf: &mut impl Buf) -> Result<Self, DecodeError> {
        let bytes = read_guarded(buf)?;
        core::str::from_utf8(&bytes)
            .map(|value| value.to_owned().into())
            .map_err(|_| DecodeError::InvalidUtf8)
    }
}
impl serde::Serialize for SecretString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self)
    }
}
impl<'de> serde::Deserialize<'de> for SecretString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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
