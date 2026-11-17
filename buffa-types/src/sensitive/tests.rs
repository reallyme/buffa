// Modified by ReallyMe Technologies Limited.
use super::*;
#[cfg(feature = "json")]
use alloc::string::ToString;
use buffa::{DecodeOptions, LazyMessageView, Message, MessageView};
use zeroize::Zeroize;

#[test]
fn temporal_wire_is_compatible_and_storage_can_be_erased() {
    let mut value = Timestamp {
        seconds: 1_700_000_123,
        nanos: 123_000_000,
    };
    let native = crate::google::protobuf::Timestamp {
        seconds: value.seconds,
        nanos: value.nanos,
        ..Default::default()
    };
    assert_eq!(value.encode_to_vec(), native.encode_to_vec());
    assert_eq!(
        Timestamp::decode_from_slice(&native.encode_to_vec()).unwrap(),
        value
    );
    value.zeroize();
    assert_eq!(value, Timestamp::default());
    assert!(DecodeOptions::new()
        .reject_unknown_fields()
        .decode_from_slice::<Timestamp>(&[0xa0, 6, 1])
        .is_err());
}

#[cfg(feature = "json")]
#[test]
fn guarded_dynamic_trees_preserve_json_and_wire_values() {
    let source = r#"{"private":[1.25,null,true,"private-string",{"nested":2.5}]}"#;
    let value: Value = serde_json::from_str(source).unwrap();
    let native = crate::google::protobuf::Value::decode_from_slice(&value.encode_to_vec()).unwrap();
    assert_eq!(
        serde_json::to_value(&native).unwrap(),
        serde_json::from_str::<serde_json::Value>(source).unwrap()
    );
    let wire = value.encode_to_bytes();
    assert_eq!(Value::decode_from_slice(&wire).unwrap(), value);
    let view = ValueView::decode_view(&wire).unwrap();
    let lazy = ValueLazyView::decode_lazy(&wire).unwrap();
    let owned = ValueOwnedView::decode(wire.clone()).unwrap();
    for json in [
        serde_json::to_string(&value).unwrap(),
        serde_json::to_string(&view).unwrap(),
        serde_json::to_string(&lazy).unwrap(),
        serde_json::to_string(&owned).unwrap(),
    ] {
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), value);
    }
    for debug in [
        alloc::format!("{value:?}"),
        alloc::format!("{view:?}"),
        alloc::format!("{lazy:?}"),
        alloc::format!("{owned:?}"),
    ] {
        assert!(!debug.contains("private-string"));
        assert!(!debug.contains("1.25"));
    }
    let mut copy = value.clone();
    copy.clear();
    assert_eq!(copy, Value::default());
}

#[cfg(feature = "json")]
#[test]
fn unset_dynamic_values_fail_export_in_every_view_family() {
    let missing = Value::default();
    assert!(serde_json::to_string(&missing).is_err());
    assert!(serde_json::to_string(&crate::google::protobuf::Value::default()).is_err());
    assert!(serde_json::to_string(&ValueView::decode_view(&[]).unwrap()).is_err());
    assert!(serde_json::to_string(&ValueLazyView::decode_lazy(&[]).unwrap()).is_err());
    assert!(
        serde_json::to_string(&ValueOwnedView::decode(buffa::bytes::Bytes::new()).unwrap())
            .is_err()
    );
    assert_eq!(serde_json::to_string(&Value::null()).unwrap(), "null");
    let mut object = Struct::default();
    object.insert("field", Value::default());
    assert!(serde_json::to_string(&object).is_err());
    let mut list = ListValue::default();
    list.push(Value::default());
    assert!(serde_json::to_string(&list).is_err());
}

#[cfg(feature = "json")]
#[test]
fn null_value_collection_elements_are_retained_as_numeric_zero() {
    type Null = buffa::EnumValue<crate::google::protobuf::NullValue>;
    let values: buffa::sensitive::List<Null> = buffa::json_helpers::enum_list::open::deserialize(
        &mut serde_json::Deserializer::from_str("[null,null]"),
    )
    .unwrap();
    assert_eq!(values.len(), 2);
    assert!(values.iter().all(|value| value.to_i32() == 0));
    let values: buffa::sensitive::Map<i32, Null> =
        buffa::json_helpers::guarded_enum::map_open::deserialize(
            &mut serde_json::Deserializer::from_str(r#"{"01":null}"#),
        )
        .unwrap();
    assert_eq!(values.get(&1).unwrap().to_i32(), 0);
}

#[cfg(feature = "json")]
#[test]
fn temporal_json_is_canonical_and_rejects_invalid_values() {
    for source in [
        r#""1970-01-01T00:00:00Z""#,
        r#""2026-01-02T03:04:05.123456789Z""#,
        r#""0001-01-01T00:00:00Z""#,
        r#""9999-12-31T23:59:59.999999999Z""#,
    ] {
        let value: Timestamp = serde_json::from_str(source).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), source);
    }
    for source in [r#""0s""#, r#""-0.001s""#, r#""315576000000.999999999s""#] {
        let value: Duration = serde_json::from_str(source).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), source);
    }
    for source in [
        r#""private-invalid-timestamp""#,
        r#""2026-99-99T00:00:00Z""#,
    ] {
        let error = serde_json::from_str::<Timestamp>(source)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("private-invalid"));
    }
    assert!(serde_json::to_string(&Timestamp {
        seconds: i64::MAX,
        nanos: 0
    })
    .is_err());
    assert!(serde_json::to_string(&Duration {
        seconds: 1,
        nanos: -1
    })
    .is_err());
    assert!(serde_json::to_string(&Value::number(f64::NAN)).is_err());
    assert!(serde_json::from_str::<Value>(r#"[1.25,2.5,"unterminated]"#).is_err());
    let nested = alloc::format!("{}0{}", "[".repeat(160), "]".repeat(160));
    assert!(serde_json::from_str::<Value>(&nested).is_err());
}
