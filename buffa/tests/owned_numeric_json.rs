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
    // Streaming JSON borrows its input; that storage remains the caller's.
    let input = String::from("\"17\"");
    let mut decoder = serde_json::Deserializer::from_str(&input);
    assert_eq!(
        buffa::json_helpers::int64::deserialize(&mut decoder).unwrap(),
        17
    );
    assert_eq!(input, "\"17\"");
}
