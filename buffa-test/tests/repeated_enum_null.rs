//! Null is a whole-container spelling, or an explicit NullValue element.
use buffa_test::enum_nulls::{Custom, Guarded, Plain};

#[test]
fn repeated_ordinary_enums_reject_first_and_later_nulls() {
    for ignore in [false, true] {
        let options = buffa::json::JsonParseOptions::new().ignore_unknown_enum_values(ignore);
        buffa::json::with_json_parse_options(&options, || {
            for input in [r#"{"values":[null]}"#, r#"{"values":[1,null,0]}"#] {
                assert!(serde_json::from_str::<Plain>(input).is_err());
                assert!(serde_json::from_str::<Custom>(input).is_err());
                assert!(serde_json::from_str::<Guarded>(input).is_err());
                let value: serde_json::Value = serde_json::from_str(input).unwrap();
                assert!(serde_json::from_value::<Plain>(value.clone()).is_err());
                assert!(serde_json::from_value::<Custom>(value.clone()).is_err());
                assert!(serde_json::from_value::<Guarded>(value).is_err());
                let impostor = input.replace("values", "impostors");
                assert!(serde_json::from_str::<Plain>(&impostor).is_err());
                assert!(serde_json::from_str::<Custom>(&impostor).is_err());
                assert!(serde_json::from_str::<Guarded>(&impostor).is_err());
            }
        });
    }
}

#[test]
fn whole_field_null_and_explicit_null_value_elements_are_preserved() {
    for ignore in [false, true] {
        let options = buffa::json::JsonParseOptions::new().ignore_unknown_enum_values(ignore);
        buffa::json::with_json_parse_options(&options, || {
            for input in [
                r#"{"values":null,"nulls":null}"#,
                r#"{"values":[],"nulls":[]}"#,
            ] {
                assert_eq!(
                    serde_json::from_str::<Plain>(input).unwrap(),
                    Plain::default()
                );
                assert_eq!(
                    serde_json::from_str::<Custom>(input).unwrap(),
                    Custom::default()
                );
                assert_eq!(
                    serde_json::from_str::<Guarded>(input).unwrap(),
                    Guarded::default()
                );
            }
            let alternate = r#"{"nulls":[null,0,"NULL_VALUE",99]}"#;
            let expected_nulls = serde_json::json!({"nulls":[null,null,null,99]});
            assert_eq!(
                serde_json::to_value(serde_json::from_str::<Plain>(alternate).unwrap()).unwrap(),
                expected_nulls
            );
            assert_eq!(
                serde_json::to_value(serde_json::from_str::<Custom>(alternate).unwrap()).unwrap(),
                expected_nulls
            );
            assert_eq!(
                serde_json::to_value(serde_json::from_str::<Guarded>(alternate).unwrap()).unwrap(),
                expected_nulls
            );
            let input = r#"{"values":[1,0,99],"nulls":[null,null]}"#;
            let expected: serde_json::Value = serde_json::from_str(input).unwrap();
            let a: Plain = serde_json::from_str(input).unwrap();
            let b: Custom = serde_json::from_str(input).unwrap();
            let c: Guarded = serde_json::from_str(input).unwrap();
            assert_eq!(a.values.len(), 3);
            assert_eq!(a.nulls.len(), 2);
            assert_eq!(b.nulls.len(), 2);
            assert_eq!(c.nulls.len(), 2);
            // Known ordinary enum values use names; unknown numbers survive.
            let mut expected = expected;
            expected["values"] = serde_json::json!(["READY", "ZERO", 99]);
            assert_eq!(serde_json::to_value(a).unwrap(), expected);
            assert_eq!(serde_json::to_value(b).unwrap(), expected);
            assert_eq!(serde_json::to_value(c).unwrap(), expected);
        });
    }
}

#[test]
fn owned_and_borrowed_null_value_json_agree() {
    use buffa::{Message, MessageView};
    use buffa_test::enum_nulls::{CustomView, GuardedView, PlainView};
    let input = r#"{"values":[1,99],"nulls":[null,99]}"#;
    let a: Plain = serde_json::from_str(input).unwrap();
    let b: Custom = serde_json::from_str(input).unwrap();
    let c: Guarded = serde_json::from_str(input).unwrap();
    let a_bytes = a.encode_to_vec();
    let b_bytes = b.encode_to_vec();
    let c_bytes = c.encode_to_vec();
    assert_eq!(
        serde_json::to_value(PlainView::decode_view(&a_bytes).unwrap()).unwrap(),
        serde_json::to_value(a).unwrap()
    );
    assert_eq!(
        serde_json::to_value(CustomView::decode_view(&b_bytes).unwrap()).unwrap(),
        serde_json::to_value(b).unwrap()
    );
    assert_eq!(
        serde_json::to_value(GuardedView::decode_view(&c_bytes).unwrap()).unwrap(),
        serde_json::to_value(c).unwrap()
    );
}
