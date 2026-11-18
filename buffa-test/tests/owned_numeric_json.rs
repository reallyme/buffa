//! Generated numeric fields use the same transferred-token ownership boundary.
#[path = "../../buffa/tests/support/owned_numeric.rs"]
mod owned_numeric;
use owned_numeric::observe;

#[test]
fn generated_numeric_contexts_wipe_transferred_tokens() {
    use buffa_test::cleanup_input::NumericContexts;
    owned_numeric::assert_detects_unwiped_drop();
    for field in [
        "optionalNumber",
        "repeatedNumbers",
        "mappedNumbers",
        "oneofNumber",
    ] {
        for (token, expected) in [
            ("17", true),
            ("9223372036854775808", false),
            ("private-numeric-token", false),
            ("", false),
        ] {
            for later_error in [false, true] {
                observe(token, expected && !later_error, |text| {
                    let value = serde_json::Value::String(text);
                    let value = match field {
                        "repeatedNumbers" => serde_json::Value::Array(vec![value]),
                        "mappedNumbers" => {
                            let mut map = serde_json::Map::new();
                            map.insert("1".into(), value);
                            serde_json::Value::Object(map)
                        }
                        _ => value,
                    };
                    let mut object = serde_json::Map::new();
                    object.insert(field.into(), value);
                    if later_error {
                        object.insert("zz_unknown".into(), serde_json::Value::Null);
                    }
                    serde_json::from_value::<NumericContexts>(serde_json::Value::Object(object))
                        .is_ok()
                });
            }
        }
    }
}
