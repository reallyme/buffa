//! Numeric helper ownership across Serde backends.
#![cfg(all(feature = "json", feature = "zeroize"))]
#[path = "support/owned_numeric.rs"]
mod owned_numeric;
use owned_numeric::observe;
use serde::de::IntoDeserializer;

#[test]
fn numeric_helpers_wipe_owned_tokens_on_success_and_failure() {
    owned_numeric::assert_detects_unwiped_drop();

    macro_rules! check {
        ($module:ident, $valid:expr, $invalid:expr) => {
            for (token, expected) in [
                ($valid, true),
                ($invalid, false),
                ("private-numeric-token", false),
                ("", false),
            ] {
                observe(token, expected, |text| {
                    buffa::json_helpers::$module::deserialize(
                        serde::de::value::StringDeserializer::<serde::de::value::Error>::new(text),
                    )
                    .is_ok()
                });
                observe(token, expected, |text| {
                    buffa::json_helpers::$module::deserialize(
                        serde_json::Value::String(text).into_deserializer(),
                    )
                    .is_ok()
                });
            }
        };
    }
    check!(int32, "17", "2147483648");
    check!(uint32, "17", "-1");
    check!(int64, "9223372036854775807", "9223372036854775808");
    check!(uint64, "18446744073709551615", "-1");
    check!(float, "1.25", "1e100");
    check!(double, "1.25", "1e1000");
    for token in ["GREEN", "PRIVATE-ENUM-NAME"] {
        for route in 0..3 {
            observe(token, token == "GREEN", |text| {
                let decoder =
                    serde::de::value::StringDeserializer::<serde::de::value::Error>::new(text);
                if route == 0 {
                    buffa::json_helpers::closed_enum::deserialize::<Color, _>(decoder).is_ok()
                } else if route == 1 {
                    buffa::json_helpers::proto_enum::deserialize::<Color, _>(decoder).is_ok()
                } else {
                    <buffa::EnumValue<Color> as serde::Deserialize>::deserialize(decoder).is_ok()
                }
            });
        }
    }
    observe("PRIVATE-REJECTED-TOKEN", false, |text| {
        use serde::de::Visitor;
        Reject.visit_string::<serde::de::value::Error>(text).is_ok()
    });
    owned_numeric::observe_bytes(|bytes| {
        use serde::de::Visitor;
        Reject
            .visit_byte_buf::<serde::de::value::Error>(bytes)
            .is_err()
    });
    for route in 0..3 {
        owned_numeric::observe_bytes(|bytes| {
            let decoder = OwnedBytes(bytes);
            match route {
                0 => buffa::json_helpers::closed_enum::deserialize::<Color, _>(decoder).is_err(),
                1 => buffa::json_helpers::proto_enum::deserialize::<Color, _>(decoder).is_err(),
                _ => <buffa::EnumValue<Color> as serde::Deserialize>::deserialize(decoder).is_err(),
            }
        });
    }
    // Streaming JSON borrows its input; that storage remains the caller's.
    let input = String::from("\"17\"");
    let mut decoder = serde_json::Deserializer::from_str(&input);
    assert_eq!(
        buffa::json_helpers::int64::deserialize(&mut decoder).unwrap(),
        17
    );
    assert_eq!(input, "\"17\"");
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Color {
    #[default]
    Green,
}
impl buffa::Enumeration for Color {
    fn from_i32(v: i32) -> Option<Self> {
        (v == 0).then_some(Self::Green)
    }
    fn to_i32(&self) -> i32 {
        0
    }
    fn proto_name(&self) -> &'static str {
        "GREEN"
    }
    fn from_proto_name(v: &str) -> Option<Self> {
        (v == "GREEN").then_some(Self::Green)
    }
}
struct Reject;
impl<'de> serde::de::Visitor<'de> for Reject {
    type Value = ();
    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("an integer")
    }
    buffa::json_helpers::reject_other_json_kinds!(str, string, bytes, byte_buf);
}

struct OwnedBytes(Vec<u8>);
impl<'de> serde::Deserializer<'de> for OwnedBytes {
    type Error = serde::de::value::Error;
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_byte_buf(self.0)
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }
}
