// Modified by ReallyMe Technologies Limited.
//! Decode map keys into their declared representation without a String stage.
//! Keys retain the ordinary protobuf map helpers' string parsing grammar.

pub use super::proto_map::serialize;
use crate::map_codec::MapStorage;
use core::{marker::PhantomData, str::FromStr};
use serde::{
    de::{MapAccess, Visitor},
    Deserializer,
};

/// Stream a map into its cleanup owner, retaining partial-key cleanup on errors.
///
/// # Errors
/// Returns the deserializer's error for malformed keys or values, including
/// null values rejected by the element's protobuf JSON implementation.
pub fn deserialize<'de, C, D>(input: D) -> Result<C, D::Error>
where
    C: MapStorage + Default,
    C::Key: FromStr,
    C::Value: super::ProtoElemJson,
    D: Deserializer<'de>,
{
    struct Map<C>(PhantomData<fn() -> C>);
    impl<'de, C> Visitor<'de> for Map<C>
    where
        C: MapStorage + Default,
        C::Key: FromStr,
        C::Value: super::ProtoElemJson,
    {
        type Value = C;
        fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("a protobuf map or null")
        }
        fn visit_unit<Error>(self) -> Result<C, Error> {
            Ok(C::default())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<C, A::Error> {
            let mut out = C::default();
            while let Some(key) =
                input.next_key_seed(super::guarded_key::KeySeed::<C::Key>(PhantomData))?
            {
                let key = crate::erasing::Erasing::new(key, C::storage_erase_key);
                let value = input.next_value_seed(super::ProtoElemSeed::<C::Value>(PhantomData))?;
                out.storage_insert(key.into_inner(), value);
            }
            Ok(out)
        }
    }
    input.deserialize_any(Map::<C>(PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    type Values = crate::sensitive::Map<i64, u64>;
    #[test]
    fn numeric_keys_match_ordinary_from_str_and_reject_private_errors() {
        for source in [
            r#"{"-0":1}"#,
            r#"{"01":1}"#,
            r#"{"+1":1}"#,
            r#"{"-9223372036854775808":1}"#,
            r#"{"9223372036854775807":1}"#,
        ] {
            let guarded =
                deserialize::<Values, _>(&mut serde_json::Deserializer::from_str(source)).unwrap();
            let ordinary: crate::__private::HashMap<i64, u64> =
                super::super::proto_map::deserialize(&mut serde_json::Deserializer::from_str(
                    source,
                ))
                .unwrap();
            assert_eq!(guarded.len(), ordinary.len());
            for (key, value) in guarded.iter() {
                assert_eq!(ordinary.get(key), Some(value));
            }
            let owned = deserialize::<Values, _>(
                serde_json::from_str::<serde_json::Value>(source).unwrap(),
            )
            .unwrap();
            assert_eq!(owned, guarded);
        }
        for source in [r#"{"private-key":1}"#, r#"{"9223372036854775808":1}"#] {
            let error = deserialize::<Values, _>(&mut serde_json::Deserializer::from_str(source))
                .unwrap_err();
            assert!(error.to_string().contains("invalid protobuf map key"));
            assert!(!error.to_string().contains("private-key"));
            assert!(!error.to_string().contains("9223372036854775808"));
        }
    }
}
