//! Unknown field preservation for round-trip fidelity.

use crate::encode_sink::EncodeSink;
use alloc::vec::Vec;

/// A collection of unknown fields encountered during decoding.
///
/// When a message is decoded with a schema that doesn't include all fields
/// present on the wire, the unknown fields are stored here so they can be
/// re-encoded without data loss.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct UnknownFields {
    fields: Vec<UnknownField>,
}

impl UnknownFields {
    /// Creates an empty set of unknown fields.
    ///
    /// `const` so an empty set can be a `static` (e.g. the default
    /// [`ReflectMessage::unknown_fields`] return value for implementations
    /// that don't preserve unknown fields).
    ///
    /// [`ReflectMessage::unknown_fields`]: https://docs.rs/buffa-descriptor/latest/buffa_descriptor/reflect/trait.ReflectMessage.html#method.unknown_fields
    pub const fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Returns `true` if no unknown fields have been recorded.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Returns the number of unknown fields recorded.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Appends an unknown field.
    pub fn push(&mut self, field: UnknownField) {
        self.fields.push(field);
    }

    /// Returns an iterator over the recorded unknown fields.
    pub fn iter(&self) -> core::slice::Iter<'_, UnknownField> {
        self.fields.iter()
    }

    /// Removes all recorded unknown fields.
    pub fn clear(&mut self) {
        self.fields.clear();
    }

    /// Retain only the fields for which the predicate returns `true`.
    ///
    /// Used by [`ExtensionSet::set_extension`](crate::ExtensionSet::set_extension)
    /// and [`clear_extension`](crate::ExtensionSet::clear_extension) to remove
    /// prior occurrences at a given field number.
    pub fn retain(&mut self, f: impl FnMut(&UnknownField) -> bool) {
        self.fields.retain(f);
    }

    /// Compute the encoded size of all unknown fields.
    pub fn encoded_len(&self) -> usize {
        self.fields.iter().map(|f| f.encoded_len()).sum()
    }

    /// Re-encode all unknown fields to `buf` in their original wire format.
    pub fn write_to(&self, buf: &mut impl EncodeSink) {
        for field in &self.fields {
            field.write_to(buf);
        }
    }

    /// Decode a concatenation of wire-format fields into [`UnknownFields`].
    ///
    /// Reads tag/data pairs until `data` is exhausted. Each field is decoded
    /// via [`decode_unknown_field`](crate::encoding::decode_unknown_field)
    /// with the full [`RECURSION_LIMIT`](crate::message::RECURSION_LIMIT)
    /// depth budget and a fresh
    /// [`DEFAULT_UNKNOWN_FIELD_LIMIT`](crate::DEFAULT_UNKNOWN_FIELD_LIMIT)
    /// unknown-field allowance.
    ///
    /// Used by [`GroupCodec`](crate::extension::codecs::GroupCodec) to turn a
    /// message's encoded bytes back into the inner-field representation that
    /// [`UnknownFieldData::Group`] wraps.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`](crate::DecodeError) if `data` contains a
    /// malformed tag, truncated field, or exceeds the recursion limit or
    /// unknown-field limit.
    pub fn decode_from_slice(mut data: &[u8]) -> Result<Self, crate::DecodeError> {
        use crate::encoding::{decode_unknown_field, Tag};
        use crate::message::{DecodeContext, DEFAULT_UNKNOWN_FIELD_LIMIT, RECURSION_LIMIT};
        let limit = core::cell::Cell::new(DEFAULT_UNKNOWN_FIELD_LIMIT);
        let ctx = DecodeContext::new(RECURSION_LIMIT, &limit);
        let mut out = Self::new();
        while !data.is_empty() {
            let tag = Tag::decode(&mut data)?;
            out.push(decode_unknown_field(tag, &mut data, ctx)?);
        }
        Ok(out)
    }
}

impl<'a> IntoIterator for &'a UnknownFields {
    type Item = &'a UnknownField;
    type IntoIter = core::slice::Iter<'a, UnknownField>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.iter()
    }
}

impl IntoIterator for UnknownFields {
    type Item = UnknownField;
    type IntoIter = alloc::vec::IntoIter<UnknownField>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.into_iter()
    }
}

impl Extend<UnknownField> for UnknownFields {
    /// Appends each field, in iteration order. A field whose number is already
    /// present is kept as a further record, as [`push`](UnknownFields::push)
    /// does.
    fn extend<T: IntoIterator<Item = UnknownField>>(&mut self, iter: T) {
        self.fields.extend(iter);
    }
}

impl FromIterator<UnknownField> for UnknownFields {
    /// Collects unknown fields into an [`UnknownFields`], keeping their order
    /// and any repeated field numbers.
    fn from_iter<T: IntoIterator<Item = UnknownField>>(iter: T) -> Self {
        Self {
            fields: iter.into_iter().collect(),
        }
    }
}

/// A single unknown field (field number + wire data).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnknownField {
    pub number: u32,
    pub data: UnknownFieldData,
}

impl UnknownField {
    /// Compute the encoded size of this field (tag + data).
    pub fn encoded_len(&self) -> usize {
        let tag_len =
            crate::encoding::varint_len(((self.number as u64) << 3) | self.data.wire_type_value());
        tag_len + self.data.encoded_len(self.number)
    }

    /// Re-encode this field (tag + data) to `buf` in its original wire format.
    pub fn write_to(&self, buf: &mut impl EncodeSink) {
        use crate::encoding::encode_varint;
        let tag_value = ((self.number as u64) << 3) | self.data.wire_type_value();
        encode_varint(tag_value, buf);
        match &self.data {
            UnknownFieldData::Varint(v) => encode_varint(*v, buf),
            UnknownFieldData::Fixed64(v) => buf.put_u64_le(*v),
            UnknownFieldData::Fixed32(v) => buf.put_u32_le(*v),
            UnknownFieldData::LengthDelimited(data) => {
                encode_varint(data.len() as u64, buf);
                buf.put_slice(data);
            }
            UnknownFieldData::Group(fields) => {
                fields.write_to(buf);
                // End-group tag (wire type 4, same field number).
                encode_varint(((self.number as u64) << 3) | 4, buf);
            }
        }
    }
}

/// The wire data for an unknown field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnknownFieldData {
    Varint(u64),
    Fixed64(u64),
    Fixed32(u32),
    LengthDelimited(Vec<u8>),
    Group(UnknownFields),
}

impl UnknownFieldData {
    fn wire_type_value(&self) -> u64 {
        match self {
            Self::Varint(_) => 0,
            Self::Fixed64(_) => 1,
            Self::LengthDelimited(_) => 2,
            Self::Group(_) => 3,
            Self::Fixed32(_) => 5,
        }
    }

    fn encoded_len(&self, field_number: u32) -> usize {
        match self {
            Self::Varint(v) => crate::encoding::varint_len(*v),
            Self::Fixed64(_) => 8,
            Self::Fixed32(_) => 4,
            Self::LengthDelimited(data) => {
                crate::encoding::varint_len(data.len() as u64) + data.len()
            }
            Self::Group(fields) => {
                // Group content + end-group tag (wire type 4, same field number).
                let end_tag_len = crate::encoding::varint_len((field_number as u64) << 3 | 4);
                fields.encoded_len() + end_tag_len
            }
        }
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for UnknownFields {
    fn arbitrary(_u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(UnknownFields::new())
    }
}
