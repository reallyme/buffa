// Modified by ReallyMe Technologies Limited.
//! Streaming enum sequences for custom repeated collections.
//! Parsed enum tokens enter their destination directly; no Value tree or Vec
//! stages identifying numbers while a later token is being decoded.
use core::marker::PhantomData;
use serde::{
    de::{DeserializeSeed, Visitor},
    Deserializer,
};

#[derive(Debug, thiserror::Error)]
enum ErrorReason {
    #[error("unknown protobuf enum value")]
    Unknown,
    #[error("protobuf enum number is outside i32 range")]
    Range,
    #[error("null is not an enum collection element")]
    Null,
}

#[derive(Clone, Copy)]
enum NullPolicy {
    Reject,
    Zero,
    #[cfg(feature = "zeroize")]
    Absent,
}

#[derive(Clone, Copy)]
pub(super) struct EnumSeed<E> {
    closed: bool,
    ignore_unknown_numbers: bool,
    null_policy: NullPolicy,
    marker: PhantomData<fn() -> E>,
}
impl<E> EnumSeed<E> {
    pub(super) fn new(closed: bool) -> Self {
        Self {
            closed,
            ignore_unknown_numbers: true,
            null_policy: NullPolicy::Reject,
            marker: PhantomData,
        }
    }
    pub(super) fn open_collection() -> Self {
        Self {
            null_policy: NullPolicy::Zero,
            ..Self::new(false)
        }
    }
    #[cfg(feature = "zeroize")]
    pub(super) fn singular_closed() -> Self {
        Self {
            ignore_unknown_numbers: false,
            ..Self::new(true)
        }
    }
    #[cfg(feature = "zeroize")]
    pub(super) fn optional(mut self) -> Self {
        self.null_policy = NullPolicy::Absent;
        self
    }
}
impl<'de, E: crate::Enumeration> DeserializeSeed<'de> for EnumSeed<E> {
    type Value = Option<crate::EnumValue<E>>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        struct Token<E>(EnumSeed<E>);
        impl<E: crate::Enumeration> Token<E> {
            fn number<Error: serde::de::Error>(
                &self,
                number: i32,
            ) -> Result<Option<crate::EnumValue<E>>, Error> {
                match E::from_i32(number) {
                    Some(value) => Ok(Some(crate::EnumValue::Known(value))),
                    None if !self.0.closed => Ok(Some(crate::EnumValue::Unknown(number))),
                    None if self.0.ignore_unknown_numbers
                        && crate::json::ignore_unknown_enum_values() =>
                    {
                        Ok(None)
                    }
                    None => Err(Error::custom(ErrorReason::Unknown)),
                }
            }
        }
        impl<E: crate::Enumeration> Visitor<'_> for Token<E> {
            type Value = Option<crate::EnumValue<E>>;
            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a protobuf enum name or integer")
            }
            fn visit_unit<Error: serde::de::Error>(self) -> Result<Self::Value, Error> {
                match self.0.null_policy {
                    NullPolicy::Zero => Ok(Some(crate::EnumValue::from(0))),
                    #[cfg(feature = "zeroize")]
                    NullPolicy::Absent => Ok(None),
                    NullPolicy::Reject => Err(Error::custom(ErrorReason::Null)),
                }
            }
            fn visit_none<Error: serde::de::Error>(self) -> Result<Self::Value, Error> {
                self.visit_unit()
            }
            fn visit_i64<Error: serde::de::Error>(self, number: i64) -> Result<Self::Value, Error> {
                self.number(i32::try_from(number).map_err(|_| Error::custom(ErrorReason::Range))?)
            }
            fn visit_u64<Error: serde::de::Error>(self, number: u64) -> Result<Self::Value, Error> {
                self.number(i32::try_from(number).map_err(|_| Error::custom(ErrorReason::Range))?)
            }
            fn visit_f64<Error: serde::de::Error>(
                self,
                _number: f64,
            ) -> Result<Self::Value, Error> {
                Err(Error::custom(super::EnumJsonErrorReason::InvalidType))
            }
            fn visit_bool<Error: serde::de::Error>(self, _: bool) -> Result<Self::Value, Error> {
                Err(Error::custom(super::EnumJsonErrorReason::InvalidType))
            }
            fn visit_str<Error: serde::de::Error>(self, name: &str) -> Result<Self::Value, Error> {
                match E::from_proto_name(name) {
                    Some(value) => Ok(Some(crate::EnumValue::Known(value))),
                    None if crate::json::ignore_unknown_enum_values() => Ok(None),
                    None => Err(Error::custom(ErrorReason::Unknown)),
                }
            }
            fn visit_string<Error: serde::de::Error>(
                self,
                name: alloc::string::String,
            ) -> Result<Self::Value, Error> {
                #[cfg(feature = "zeroize")]
                let name = zeroize::Zeroizing::new(name);
                self.visit_str(&name)
            }
        }
        deserializer.deserialize_any(Token(self))
    }
}

/// Protobuf JSON enum arrays decoded directly into an open-enum collection.
pub mod open {
    pub use super::super::repeated_enum::serialize;
    use super::*;
    /// Decode an enum array or null, applying the configured unknown-name policy.
    /// # Errors
    /// Null elements retain the ordinary open-enum mapping to numeric zero.
    /// Returns the deserializer's error for malformed or out-of-range elements.
    pub fn deserialize<
        'de,
        E: crate::Enumeration,
        C: crate::ProtoList<crate::EnumValue<E>>,
        D: Deserializer<'de>,
    >(
        deserializer: D,
    ) -> Result<C, D::Error> {
        struct Sequence<E, C>(PhantomData<fn() -> (E, C)>);
        impl<'de, E: crate::Enumeration, C: crate::ProtoList<crate::EnumValue<E>>> Visitor<'de>
            for Sequence<E, C>
        {
            type Value = C;
            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("an enum array or null")
            }
            fn visit_unit<Error>(self) -> Result<C, Error> {
                Ok(C::default())
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut source: A) -> Result<C, A::Error> {
                let mut out = C::default();
                while let Some(value) = source.next_element_seed(EnumSeed::<E>::open_collection())?
                {
                    if let Some(value) = value {
                        crate::ProtoList::push(&mut out, value);
                    }
                }
                Ok(out)
            }
        }
        deserializer.deserialize_any(Sequence::<E, C>(PhantomData))
    }
}

/// Protobuf JSON enum arrays decoded directly into a closed-enum collection.
pub mod closed {
    pub use super::super::repeated_closed_enum::serialize;
    use super::*;
    /// Decode an enum array or null, applying the configured unknown-value policy.
    /// # Errors
    /// Returns the deserializer's error for malformed, null or unrecognized elements.
    pub fn deserialize<'de, E: crate::Enumeration, C: crate::ProtoList<E>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<C, D::Error> {
        struct Sequence<E, C>(PhantomData<fn() -> (E, C)>);
        impl<'de, E: crate::Enumeration, C: crate::ProtoList<E>> Visitor<'de> for Sequence<E, C> {
            type Value = C;
            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("an enum array or null")
            }
            fn visit_unit<Error>(self) -> Result<C, Error> {
                Ok(C::default())
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut source: A) -> Result<C, A::Error> {
                let mut out = C::default();
                while let Some(value) = source.next_element_seed(EnumSeed::<E>::new(true))? {
                    if let Some(crate::EnumValue::Known(value)) = value {
                        crate::ProtoList::push(&mut out, value);
                    }
                }
                Ok(out)
            }
        }
        deserializer.deserialize_any(Sequence::<E, C>(PhantomData))
    }
}
