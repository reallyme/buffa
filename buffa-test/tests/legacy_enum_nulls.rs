//! Frozen pre-change bindings exercise compatibility with the updated WKT.
//! Captured with generator revision ce3fd6e using json=true, views=true and
//! unknown_fields=false; the input schema is beside the frozen bindings. Keep
//! their field/view plumbing unchanged so this exercises existing consumers.
#[allow(clippy::derivable_impls)]
pub mod legacy {
    include!("fixtures/legacy_enum_nulls/legacy_enum_nulls.mod.rs");
}

#[test]
fn pre_change_generated_fields_and_views_retain_null_value_semantics() {
    use buffa::{Message, MessageView};
    use legacy::{Legacy, LegacyView};
    let input = r#"{"values":[null,99],"byId":{"7":"NULL_VALUE","8":99}}"#;
    let value: Legacy = serde_json::from_str(input).unwrap();
    let expected: serde_json::Value = serde_json::from_str(input).unwrap();
    let bytes = value.encode_to_vec();
    let view = LegacyView::decode_view(&bytes).unwrap();
    assert_eq!(serde_json::to_value(&value).unwrap(), expected);
    assert_eq!(serde_json::to_value(&view).unwrap(), expected);
}

#[test]
fn pre_change_optional_null_value_presence_is_unchanged() {
    let value = legacy::Legacy {
        optional_value: Some(buffa_types::google::protobuf::NullValue::NULL_VALUE.into()),
        ..Default::default()
    };
    use buffa::{Message, MessageView};
    let bytes = value.encode_to_vec();
    let view = legacy::LegacyView::decode_view(&bytes).unwrap();
    for json in [
        serde_json::to_string(&value).unwrap(),
        serde_json::to_string(&view).unwrap(),
    ] {
        assert!(json.contains("NULL_VALUE"));
        let decoded: legacy::Legacy = serde_json::from_str(&json).unwrap();
        assert!(decoded.optional_value.is_some());
    }
}
