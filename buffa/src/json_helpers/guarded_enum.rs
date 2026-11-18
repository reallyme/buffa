// Modified by ReallyMe Technologies Limited.
//! Guarded enum fields reject malformed values before building a JSON tree.
//! Owned enum-name tokens are wiped by the shared streaming seed.

use super::enum_list::EnumSeed;
use crate::{map_codec::MapStorage, EnumValue, Enumeration};
use core::{marker::PhantomData, str::FromStr};
use serde::{
    de::{DeserializeSeed, MapAccess, Visitor},
    Deserializer,
};

fn optional<'de, E: Enumeration, D: Deserializer<'de>>(
    input: D,
    seed: EnumSeed<E>,
) -> Result<Option<EnumValue<E>>, D::Error> {
    // Receive owned tokens directly too: a nullable wrapper that only handles
    // visit_some can reject and drop a transferred String before the enum seed
    // gets a chance to establish its wipe owner.
    seed.optional().deserialize(input)
}

/// Singular open enums, with null and ignored names mapping to zero.
pub mod open {
    pub use super::super::proto_enum::serialize;
    use super::*;
    /// Decode a scalar enum directly without owned JSON scratch.
    /// # Errors
    /// Rejects malformed tokens and unknown names with fixed diagnostics.
    pub fn deserialize<'de, E: Enumeration, D: Deserializer<'de>>(
        input: D,
    ) -> Result<EnumValue<E>, D::Error> {
        optional(input, EnumSeed::new(false))
            .map(|value| value.unwrap_or_else(|| EnumValue::from(0)))
    }
}

/// Singular closed enums, preserving strict numeric validation.
pub mod closed {
    pub use super::super::closed_enum::serialize;
    use super::*;
    /// Decode a closed scalar enum directly; null uses its default.
    /// # Errors
    /// Rejects malformed tokens, unknown numbers and strict unknown names.
    pub fn deserialize<'de, E: Enumeration + Default, D: Deserializer<'de>>(
        input: D,
    ) -> Result<E, D::Error> {
        optional(input, EnumSeed::singular_closed()).map(|value| match value {
            Some(EnumValue::Known(value)) => value,
            _ => E::default(),
        })
    }
}

/// Explicit-presence open enums, with null and ignored names unset.
pub mod optional_open {
    pub use super::super::opt_enum::serialize;
    use super::*;
    /// Decode an optional enum directly, retaining unknown numeric values.
    /// # Errors
    /// Rejects malformed tokens and strict unknown names.
    pub fn deserialize<'de, E: Enumeration, D: Deserializer<'de>>(
        input: D,
    ) -> Result<Option<EnumValue<E>>, D::Error> {
        optional(input, EnumSeed::new(false))
    }
}

/// Explicit-presence closed enums, with null and ignored values unset.
pub mod optional_closed {
    pub use super::super::opt_closed_enum::serialize;
    use super::*;
    /// Decode an optional closed enum directly.
    /// # Errors
    /// Rejects malformed tokens and strict unknown names or numbers.
    pub fn deserialize<'de, E: Enumeration, D: Deserializer<'de>>(
        input: D,
    ) -> Result<Option<E>, D::Error> {
        optional(input, EnumSeed::new(true)).map(|value| match value {
            Some(EnumValue::Known(value)) => Some(value),
            _ => None,
        })
    }
}

struct EnumMapSeed<E: Enumeration, C, V> {
    token: EnumSeed<E>,
    project: fn(EnumValue<E>) -> Option<V>,
    marker: PhantomData<fn() -> C>,
}
impl<'de, E, C, V> Visitor<'de> for EnumMapSeed<E, C, V>
where
    E: Enumeration,
    C: MapStorage<Value = V> + Default,
    C::Key: FromStr,
{
    type Value = C;
    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a protobuf enum map or null")
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
            if let Some(value) = input.next_value_seed(self.token)?.and_then(self.project) {
                out.storage_insert(key.into_inner(), value);
            }
        }
        Ok(out)
    }
    crate::reject_other_json_kinds!(
        bool, i64, u64, i128, u128, f64, str, string, bytes, byte_buf, seq
    );
}

/// Open-enum maps streamed directly into their owning container.
pub mod map_open {
    pub use super::super::map_enum::serialize;
    use super::*;
    /// Decode enum values without buffering JSON trees; null maps to numeric zero.
    /// # Errors
    /// Rejects malformed values and strict unknown names.
    pub fn deserialize<'de, E, C, D>(input: D) -> Result<C, D::Error>
    where
        E: Enumeration,
        C: MapStorage<Value = EnumValue<E>> + Default,
        C::Key: FromStr,
        D: Deserializer<'de>,
    {
        input.deserialize_any(EnumMapSeed::<E, C, EnumValue<E>> {
            token: EnumSeed::open_map(),
            project: Some,
            marker: PhantomData,
        })
    }
}

/// Closed-enum maps streamed directly into their owning container.
pub mod map_closed {
    pub use super::super::map_closed_enum::serialize;
    use super::*;
    /// Decode enum values without buffering JSON trees; null values are rejected.
    /// # Errors
    /// Rejects malformed values and strict unknown names or numbers.
    pub fn deserialize<'de, C, D>(input: D) -> Result<C, D::Error>
    where
        C: MapStorage + Default,
        C::Value: Enumeration + Default,
        C::Key: FromStr,
        D: Deserializer<'de>,
    {
        input.deserialize_any(EnumMapSeed::<C::Value, C, C::Value> {
            token: EnumSeed::new(true),
            project: |value| match value {
                EnumValue::Known(value) => Some(value),
                _ => None,
            },
            marker: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    enum State {
        #[default]
        Unset,
        Present,
    }
    impl Enumeration for State {
        fn from_i32(number: i32) -> Option<Self> {
            match number {
                0 => Some(Self::Unset),
                1 => Some(Self::Present),
                _ => None,
            }
        }
        fn to_i32(&self) -> i32 {
            match self {
                Self::Unset => 0,
                Self::Present => 1,
            }
        }
        fn proto_name(&self) -> &'static str {
            match self {
                Self::Unset => "UNSET",
                Self::Present => "PRESENT",
            }
        }
        fn from_proto_name(name: &str) -> Option<Self> {
            match name {
                "UNSET" => Some(Self::Unset),
                "PRESENT" => Some(Self::Present),
                _ => None,
            }
        }
    }
    impl zeroize::Zeroize for State {
        fn zeroize(&mut self) {
            *self = Self::Unset;
        }
    }
    type OpenMap = crate::sensitive::Map<i32, EnumValue<State>>;
    type ClosedMap = crate::sensitive::Map<i32, State>;

    #[test]
    fn transferred_owned_tokens_reach_the_enum_seed() {
        let input = serde::de::value::StringDeserializer::<serde::de::value::Error>::new(
            String::from("private-token"),
        );
        let error = optional_closed::deserialize::<State, _>(input).unwrap_err();
        assert_eq!(error.to_string(), "unknown protobuf enum value");
    }
    #[test]
    fn scalar_nulls_and_numeric_bounds_preserve_field_semantics() {
        assert_eq!(
            open::deserialize::<State, _>(&mut serde_json::Deserializer::from_str("null")).unwrap(),
            EnumValue::Known(State::Unset)
        );
        assert_eq!(
            closed::deserialize::<State, _>(&mut serde_json::Deserializer::from_str("null"))
                .unwrap(),
            State::Unset
        );
        assert_eq!(
            optional_open::deserialize::<State, _>(&mut serde_json::Deserializer::from_str("null"))
                .unwrap(),
            None
        );
        assert_eq!(
            optional_closed::deserialize::<State, _>(&mut serde_json::Deserializer::from_str(
                "null"
            ))
            .unwrap(),
            None
        );
        assert_eq!(
            open::deserialize::<State, _>(&mut serde_json::Deserializer::from_str("37")).unwrap(),
            EnumValue::Unknown(37)
        );
        for source in ["2147483648", "-2147483649", "1.5", "{}", "[]"] {
            assert!(
                open::deserialize::<State, _>(&mut serde_json::Deserializer::from_str(source))
                    .is_err()
            );
            assert!(
                closed::deserialize::<State, _>(&mut serde_json::Deserializer::from_str(source))
                    .is_err()
            );
        }
    }
    #[test]
    fn maps_stream_values_and_reject_malformed_entries() {
        let open = map_open::deserialize::<State, OpenMap, _>(
            &mut serde_json::Deserializer::from_str(r#"{"3":"PRESENT","4":37}"#),
        )
        .unwrap();
        assert_eq!(open.get(&3), Some(&EnumValue::Known(State::Present)));
        assert_eq!(open.get(&4), Some(&EnumValue::Unknown(37)));
        let closed = map_closed::deserialize::<ClosedMap, _>(
            &mut serde_json::Deserializer::from_str(r#"{"3":"PRESENT"}"#),
        )
        .unwrap();
        assert_eq!(closed.get(&3), Some(&State::Present));
        let open = map_open::deserialize::<State, OpenMap, _>(
            &mut serde_json::Deserializer::from_str(r#"{"-0":null,"01":null,"+2":null}"#),
        )
        .unwrap();
        assert_eq!(open.len(), 3);
        for number in 0..=2 {
            assert_eq!(open.get(&number), Some(&EnumValue::Known(State::Unset)));
        }
        assert!(
            map_closed::deserialize::<ClosedMap, _>(&mut serde_json::Deserializer::from_str(
                r#"{"3":null}"#
            ),)
            .is_err()
        );
        for source in [r#"{"3":{"private":"token"}}"#, r#"{"3":"private-token"}"#] {
            let error = map_open::deserialize::<State, OpenMap, _>(
                &mut serde_json::Deserializer::from_str(source),
            )
            .unwrap_err();
            assert!(!error.to_string().contains("private"));
            assert!(map_closed::deserialize::<ClosedMap, _>(
                &mut serde_json::Deserializer::from_str(source)
            )
            .is_err());
        }
    }
    #[cfg(feature = "std")]
    #[test]
    fn lenient_optional_and_map_values_are_filtered_but_required_numbers_are_strict() {
        crate::json::with_json_parse_options(
            &crate::json::JsonParseOptions::new().ignore_unknown_enum_values(true),
            || {
                assert_eq!(
                    optional_closed::deserialize::<State, _>(
                        &mut serde_json::Deserializer::from_str("37")
                    )
                    .unwrap(),
                    None
                );
                assert!(
                    closed::deserialize::<State, _>(&mut serde_json::Deserializer::from_str("37"))
                        .is_err()
                );
                let map = map_closed::deserialize::<ClosedMap, _>(
                    &mut serde_json::Deserializer::from_str(
                        r#"{"1":37,"2":"UNKNOWN","3":"PRESENT"}"#,
                    ),
                )
                .unwrap();
                assert_eq!(map.len(), 1);
            },
        );
    }

    /// Values that must never appear in a rejection: each probe's token.
    const MARKERS: [&str; 6] = [
        "123456789",
        "1.5e",
        "true",
        "private-token",
        "340282366920938463463374607431768211455",
        "170141183460469231731687303715884105728",
    ];
    const JSON_PROBES: [&str; 6] = [
        "123456789",
        "-1.5e300",
        "true",
        r#""private-token""#,
        "[]",
        "{}",
    ];

    /// Feed every probe a guarded entry point does not accept, and require a
    /// rejection that names no probe value. `accepted` lists JSON probes the
    /// entry point takes as valid input (for example `[]` for a list).
    macro_rules! assert_rejections_redacted {
        ($accepted:expr, |$input:ident| $decode:expr) => {{
            use serde::de::value::{
                BytesDeserializer, Error, I128Deserializer, StringDeserializer, U128Deserializer,
            };
            fn check(label: &str, error: Option<String>) {
                let error = error.unwrap_or_else(|| panic!("{label} was accepted"));
                for marker in MARKERS {
                    assert!(!error.contains(marker), "{label} echoed {marker}: {error}");
                }
            }
            for source in JSON_PROBES {
                if !$accepted.contains(&source) {
                    let $input = &mut serde_json::Deserializer::from_str(source);
                    check(source, $decode.err().map(|error| error.to_string()));
                }
            }
            if !$accepted.contains(&r#""private-token""#) {
                let $input = StringDeserializer::<Error>::new(String::from("private-token"));
                check("owned string", $decode.err().map(|error| error.to_string()));
            }
            let $input = BytesDeserializer::<Error>::new(b"private-token");
            check("bytes", $decode.err().map(|error| error.to_string()));
            let $input = U128Deserializer::<Error>::new(u128::MAX);
            check("u128", $decode.err().map(|error| error.to_string()));
            let $input = I128Deserializer::<Error>::new(i128::MIN);
            check("i128", $decode.err().map(|error| error.to_string()));
        }};
    }

    #[test]
    fn guarded_visitors_never_echo_a_rejected_token() {
        use crate::json_helpers::{enum_list, guarded_map, proto_list};
        use crate::sensitive::{List, Map};
        let none: [&str; 0] = [];
        // An open enum keeps an in-range unknown number.
        assert_rejections_redacted!(["123456789"], |input| open::deserialize::<State, _>(input));
        assert_rejections_redacted!(none, |input| closed::deserialize::<State, _>(input));
        assert_rejections_redacted!(
            ["123456789"],
            |input| optional_open::deserialize::<State, _>(input)
        );
        assert_rejections_redacted!(none, |input| optional_closed::deserialize::<State, _>(
            input
        ));
        assert_rejections_redacted!(["{}"], |input| map_open::deserialize::<State, OpenMap, _>(
            input
        ));
        assert_rejections_redacted!(["{}"], |input| map_closed::deserialize::<ClosedMap, _>(
            input
        ));
        assert_rejections_redacted!(
            ["{}"],
            |input| guarded_map::deserialize::<Map<i64, u64>, _>(input)
        );
        assert_rejections_redacted!(
            ["[]"],
            |input| proto_list::deserialize::<i32, List<i32>, _>(input)
        );
        assert_rejections_redacted!(["[]"], |input| enum_list::open::deserialize::<
            State,
            List<EnumValue<State>>,
            _,
        >(input));
        assert_rejections_redacted!(["[]"], |input| enum_list::closed::deserialize::<
            State,
            List<State>,
            _,
        >(input));
        assert_rejections_redacted!(["{}"], |input| {
            <Map<i64, u64> as serde::Deserialize>::deserialize(input)
        });
        assert_rejections_redacted!(["[]"], |input| {
            <List<u64> as serde::Deserialize>::deserialize(input)
        });
    }
}
