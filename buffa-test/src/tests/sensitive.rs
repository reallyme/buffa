// Modified by ReallyMe Technologies Limited.
use crate::sensitive_test::{Record, RecordLazyView, RecordOwnedView, RecordView, Status};
use buffa::{DecodeError, DecodeOptions, LazyMessageView, Message, MessageView};
use zeroize::Zeroize;

#[test]
fn enum_declaration_overrides_support_guarded_inspection() {
    use crate::sensitive_editions::{State, States, StatesView};
    use buffa_descriptor::reflect::{InspectMessage, InspectValueRef};
    let value: States =
        serde_json::from_str(r#"{"values":["STATE_PRESENT"],"entries":{"7":"STATE_PRESENT"}}"#)
            .unwrap();
    assert_eq!(value.values[0], State::STATE_PRESENT);
    let wire = value.encode_to_vec();
    let view = StatesView::decode_view(&wire).unwrap();
    assert_eq!(view.to_owned_message().unwrap(), value);
    for inspected in [value.inspect(), view.inspect()] {
        let field = inspected.message_descriptor().field(1).unwrap();
        match inspected.get(field).unwrap() {
            InspectValueRef::List(values) => assert!(matches!(
                values.get(0),
                Some(InspectValueRef::EnumNumber(1))
            )),
            _ => panic!("expected inspected list"),
        }
    }
}

#[test]
fn guarded_enum_fields_stream_and_keep_null_and_unknown_policies() {
    use crate::sensitive_test::DeclaredDefaults;
    use buffa::json::{with_json_parse_options, JsonParseOptions};
    for source in [
        r#"{"optionalStatus":{"private":"sensitive"}}"#,
        r#"{"statusValues":{"3":{"private":"sensitive"}}}"#,
        r#"{"selectedStatus":"sensitive-enum-name"}"#,
    ] {
        let error = serde_json::from_str::<Record>(source).unwrap_err();
        assert!(!error.to_string().contains("sensitive"));
    }
    for source in [
        r#"{"optionalState":{"private":"sensitive"}}"#,
        r#"{"states":{"3":{"private":"sensitive"}}}"#,
        r#"{"state":"sensitive-enum-name"}"#,
        r#"{"selectedState":"sensitive-enum-name"}"#,
        r#"{"states":{"3":null}}"#,
    ] {
        let error = serde_json::from_str::<DeclaredDefaults>(source).unwrap_err();
        assert!(!error.to_string().contains("sensitive"));
    }
    let value: Record = serde_json::from_str(
        r#"{"optionalStatus":null,"selectedStatus":null,"statusValues":null}"#,
    )
    .unwrap();
    assert!(value.optional_status.is_none());
    assert!(value.secret.is_none());
    assert!(value.status_values.is_empty());
    let options = JsonParseOptions::new().ignore_unknown_enum_values(true);
    with_json_parse_options(&options, || {
        let value: Record = serde_json::from_str(r#"{"optionalStatus":"UNKNOWN","statusValues":{"1":"UNKNOWN","2":37},"selectedStatus":"UNKNOWN"}"#).unwrap();
        assert!(value.optional_status.is_none());
        assert!(value.secret.is_none());
        assert_eq!(
            value.status_values.get(&2),
            Some(&buffa::EnumValue::Unknown(37))
        );
        let value: DeclaredDefaults = serde_json::from_str(
            r#"{"optionalState":99,"states":{"1":"UNKNOWN","2":99,"3":"PRESENT"}}"#,
        )
        .unwrap();
        assert!(value.optional_state.is_none());
        assert_eq!(value.states.len(), 1);
        assert!(serde_json::from_str::<DeclaredDefaults>(r#"{"state":99}"#).is_err());
    });
}

#[test]
fn guarded_numeric_map_keys_decode_without_intermediate_strings() {
    let value: Record = serde_json::from_str(r#"{"numericScores":{"9223372036854775807":1.5},"numericMessages":{"9223372036854775807":{"first":2.5}}}"#).unwrap();
    assert_eq!(value.numeric_scores.get(&i64::MAX), Some(&1.5));
    assert_eq!(value.numeric_messages.get(&i64::MAX).unwrap().first, 2.5);
    assert_eq!(
        serde_json::from_str::<Record>(&serde_json::to_string(&value).unwrap()).unwrap(),
        value
    );
    for source in [
        r#"{"numericScores":{"9223372036854775807":[]}}"#,
        r#"{"numericMessages":{"9223372036854775807":{"first":1.5,"second":[]}}}"#,
    ] {
        assert!(serde_json::from_str::<Record>(source).is_err());
    }
}

#[test]
fn recursive_cleanup_preserves_proto2_presence_and_declared_defaults() {
    use crate::sensitive_test::{declared_defaults::State, DeclaredDefaults};
    let mut value: DeclaredDefaults = serde_json::from_str("{}").unwrap();
    assert_eq!(value.count, 37);
    assert_eq!(&*value.label, "configured");
    assert_eq!(&*value.data, b"initial");
    assert_eq!(value.state, State::SECOND);
    assert_eq!(value.note, None);
    let bytes = value.encode_to_vec();
    assert_eq!(DeclaredDefaults::decode_from_slice(&bytes).unwrap(), value);
    assert_eq!(
        crate::sensitive_test::DeclaredDefaultsView::decode_view(&bytes)
            .unwrap()
            .to_owned_message()
            .unwrap(),
        value
    );
    value.zeroize();
    assert_eq!(value.count, 0);
    assert!(value.label.is_empty());
    assert!(value.data.is_empty());
    value.clear();
    assert_eq!(value, DeclaredDefaults::default());
    let value: DeclaredDefaults =
        serde_json::from_str(r#"{"history":["FIRST","SECOND"]}"#).unwrap();
    assert_eq!(&*value.history, &[State::FIRST, State::SECOND]);
    assert!(serde_json::from_str::<DeclaredDefaults>(r#"{"history":["FIRST",99]}"#).is_err());

    let value: DeclaredDefaults = serde_json::from_str(r#"{"states":{"3":"PRESENT"}}"#).unwrap();
    let wire = value.encode_to_vec();
    let eager = crate::sensitive_test::DeclaredDefaultsView::decode_view(&wire).unwrap();
    let lazy = crate::sensitive_test::DeclaredDefaultsLazyView::decode_lazy(&wire).unwrap();
    assert_eq!(eager.to_owned_message().unwrap(), value);
    assert_eq!(lazy.to_owned_message().unwrap(), value);
    assert_eq!(
        serde_json::from_str::<DeclaredDefaults>(&serde_json::to_string(&eager).unwrap()).unwrap(),
        value
    );
}

#[test]
fn recursive_repeated_messages_have_guarded_owners_in_every_shape() {
    let value = Record::default().with_history(vec![Record::default()
        .with_handle(37)
        .with_history(vec![Record::default().with_handle(99)])]);
    let wire = value.encode_to_vec();
    assert_eq!(Record::decode_from_slice(&wire).unwrap(), value);
    assert_eq!(
        RecordView::decode_view(&wire)
            .unwrap()
            .to_owned_message()
            .unwrap(),
        value
    );
    assert_eq!(
        RecordLazyView::decode_lazy(&wire)
            .unwrap()
            .to_owned_message()
            .unwrap(),
        value
    );
    assert_eq!(
        serde_json::from_str::<Record>(&serde_json::to_string(&value).unwrap()).unwrap(),
        value
    );
}

#[test]
fn foreign_representations_work_across_separate_generation_calls() {
    use crate::shared_guarded::{Credentials, CredentialsLazyView, CredentialsView};
    let value: Credentials = serde_json::from_str(
        r#"{"keys":["c2VjcmV0"],"labels":["private"],"values":{"private":"c2VjcmV0"},"identity":"person","note":"detail"}"#,
    ).unwrap();
    assert_eq!(&*value.keys[0], b"secret");
    assert_eq!(&*value.labels[0], "private");
    assert_eq!(&**value.values.get("private").unwrap(), b"secret");
    let wire = value.encode_to_vec();
    assert_eq!(Credentials::decode_from_slice(&wire).unwrap(), value);
    for json in [
        serde_json::to_string(&value).unwrap(),
        serde_json::to_string(&CredentialsView::decode_view(&wire).unwrap()).unwrap(),
        serde_json::to_string(&CredentialsLazyView::decode_lazy(&wire).unwrap()).unwrap(),
    ] {
        assert_eq!(serde_json::from_str::<Credentials>(&json).unwrap(), value);
    }
    let second: crate::shared_guarded_second::Credentials =
        serde_json::from_str(r#"{"keys":["c2VjcmV0"],"labels":["private"]}"#).unwrap();
    assert_eq!(second.keys, value.keys);
    assert_eq!(second.labels, value.labels);
    assert!(serde_json::from_str::<Credentials>(r#"{"keys":["c2VjcmV0",null]}"#).is_err());
    assert!(serde_json::from_str::<Credentials>(r#"{"values":{"private":"%%%"}}"#).is_err());
}

#[test]
fn guarded_inspection_borrows_values_and_rejects_foreign_descriptors() {
    use buffa_descriptor::reflect::{
        InspectErrorReason, InspectMessage, InspectValueRef as Value, MapKeyRef,
    };
    let value = record();
    let inspect = value.inspect();
    let descriptor = inspect.message_descriptor();
    assert!(matches!(
        inspect.get(descriptor.field(1).unwrap()).unwrap(),
        Value::U64(123)
    ));
    match inspect.get(descriptor.field(6).unwrap()).unwrap() {
        Value::String(text) => assert_eq!(text.as_ptr(), value.identity.as_ptr()),
        other => panic!("unexpected kind: {other:?}"),
    }
    match inspect.get(descriptor.field(9).unwrap()).unwrap() {
        Value::List(values) => assert!(matches!(values.get(1), Some(Value::F32(2.5)))),
        other => panic!("unexpected kind: {other:?}"),
    }
    match inspect.get(descriptor.field(10).unwrap()).unwrap() {
        Value::Map(values) => assert!(matches!(
            values.get(&MapKeyRef::String("private")),
            Some(Value::F64(3.5))
        )),
        other => panic!("unexpected kind: {other:?}"),
    }
    match inspect.get(descriptor.field(11).unwrap()).unwrap() {
        Value::Message(child) => assert!(matches!(
            child
                .get(child.message_descriptor().field(1).unwrap())
                .unwrap(),
            Value::U64(456)
        )),
        other => panic!("unexpected kind: {other:?}"),
    }
    assert!(!format!("{inspect:?}").contains("private"));
    let mut count = 0;
    inspect.for_each_set(&mut |_, _| count += 1).unwrap();
    assert!(count > 0);
    let mut borrowed = Vec::new();
    inspect
        .for_each_set(&mut |field, value| {
            if let Value::String(text) = value {
                borrowed.push((field.number(), text));
            }
        })
        .unwrap();
    let (_, identity) = borrowed.iter().find(|(number, _)| *number == 6).unwrap();
    assert_eq!(identity.as_ptr(), value.identity.as_ptr());

    let foreign =
        buffa_descriptor::DescriptorPool::decode(crate::sensitive_test::FILE_DESCRIPTOR_SET_BYTES)
            .unwrap();
    let index = foreign.message_index("sensitive_test.Record").unwrap();
    let foreign_field = foreign.message(index).field(1).unwrap();
    assert!(matches!(
        inspect.get(foreign_field),
        Err(InspectErrorReason::FieldNotMember)
    ));
    assert!(!inspect.has(foreign_field));

    let wire = value.encode_to_vec();
    let eager = RecordView::decode_view(&wire).unwrap();
    let inspect = eager.inspect();
    assert!(matches!(
        inspect
            .get(inspect.message_descriptor().field(1).unwrap())
            .unwrap(),
        Value::U64(123)
    ));
}

#[cfg(feature = "arbitrary")]
#[test]
fn guarded_arbitrary_is_depth_bounded_and_produces_decodable_messages() {
    use arbitrary::{Arbitrary, Unstructured};
    use buffa::sensitive::GuardedArbitrary;
    assert!(Record::arbitrary_guarded(&mut Unstructured::new(&[]), 0).is_err());
    let mut successes = 0;
    for seed in 0u8..=255 {
        let source = [seed; 512];
        if let Ok(value) = Record::arbitrary(&mut Unstructured::new(&source)) {
            let wire = value.encode_to_vec();
            let decoded = Record::decode_from_slice(&wire).unwrap();
            // NaN values intentionally use their wire representation for equality.
            assert_eq!(decoded.encode_to_vec(), wire);
            successes += 1;
        }
    }
    assert!(successes > 0);
    assert!(
        crate::shared_guarded::Credentials::arbitrary(&mut Unstructured::new(&[0; 128])).is_ok()
    );
    assert!(buffa_types::sensitive::Value::arbitrary(&mut Unstructured::new(&[0; 128])).is_ok());
}

#[test]
fn guarded_unknown_json_diagnostics_do_not_echo_private_field_names() {
    let source = r#"{"private-identifying-field-name":"private-value"}"#;
    for error in [
        serde_json::from_str::<Record>(source).unwrap_err(),
        serde_json::from_str::<crate::sensitive_test::Empty>(source).unwrap_err(),
        serde_json::from_str::<crate::sensitive_test::DeclaredDefaults>(source).unwrap_err(),
    ] {
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("unknown JSON field in guarded message"));
        assert!(!diagnostic.contains("private-identifying"));
        assert!(!diagnostic.contains("private-value"));
    }
}

#[test]
fn view_export_denial_covers_numeric_fields_and_containing_messages() {
    use crate::sensitive_test::*;
    fn denied<T: serde::Serialize>(value: &T) {
        let mut output = Vec::new();
        let error = serde_json::to_writer(&mut output, value).unwrap_err();
        assert!(error
            .to_string()
            .contains("serialization of this view is disabled"));
        assert!(
            output.is_empty(),
            "denied views must not emit even a partial object"
        );
    }
    let value = ViewPrivate::default().with_token_handle(37);
    assert!(serde_json::to_string(&value).unwrap().contains("37"));
    let bytes = value.encode_to_vec();
    denied(&ViewPrivateView::decode_view(&bytes).unwrap());
    denied(&ViewPrivateLazyView::decode_lazy(&bytes).unwrap());
    denied(&ViewPrivateView::default());
    denied(&ViewPrivateOwnedView::decode(bytes.clone().into()).unwrap());
    denied(&buffa::OwnedView::<ViewPrivateView<'static>>::decode(bytes.into()).unwrap());

    let parent = ViewPrivateParent::default()
        .with_child(value)
        .with_count(19);
    assert!(serde_json::to_string(&parent).unwrap().contains("37"));
    let bytes = parent.encode_to_vec();
    denied(&ViewPrivateParentView::decode_view(&bytes).unwrap());
    denied(&ViewPrivateParentLazyView::decode_lazy(&bytes).unwrap());
    denied(&ViewPrivateParentOwnedView::decode(bytes.into()).unwrap());

    let map: ViewPrivateMap =
        serde_json::from_str(r#"{"values":{"private":{"tokenHandle":"37"}}}"#).unwrap();
    let bytes = map.encode_to_vec();
    denied(&ViewPrivateMapView::decode_view(&bytes).unwrap());
    denied(&ViewPrivateMapLazyView::decode_lazy(&bytes).unwrap());
    denied(&ViewPrivateMapOwnedView::decode(bytes.into()).unwrap());

    // Denial runs before materializing a malformed lazy child too.
    let lazy = ViewPrivateParentLazyView::decode_lazy(&[10, 1, 0xff]).unwrap();
    denied(&lazy);
}

fn record() -> Record {
    serde_json::from_str(r#"{"handle":"123","index":7,"enabled":true,"score":1.5,"status":"STATUS_VALID","identity":"private","key":"c2VjcmV0","sequence":"9","embedding":[1.25,2.5],"scores":{"private":3.5},"child":{"handle":"456","count":"77"},"label":"secret","createdAt":"2026-01-02T03:04:05.123Z","claims":{"person":"private","values":[1.25,null,true]},"statusHistory":["STATUS_VALID",19]}"#).unwrap()
}

#[test]
fn scalar_guards_and_collections_roundtrip_and_clear() {
    let mut value = record();
    let wire = value.encode_to_vec();
    assert_eq!(Record::decode_from_slice(&wire).unwrap(), value);
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(serde_json::from_str::<Record>(&json).unwrap(), value);
    assert_eq!(value.status, buffa::EnumValue::from(Status::STATUS_VALID));
    value.zeroize();
    assert_eq!(value, Record::default());
    let mut value = record();
    value.clear();
    assert_eq!(value, Record::default());
}

#[test]
fn cleanup_enabled_json_errors_and_replacements_are_consistent() {
    let value:Record=serde_json::from_str(r#"{"handle":"123","handle":"456","identity":"old","identity":"new","scores":{"same":1.25,"same":2.5}}"#).unwrap();
    assert_eq!(value.handle, 456);
    assert_eq!(&*value.identity, "new");
    assert_eq!(value.scores.get("same"), Some(&2.5));
    for json in [
        r#"{"handle":"123","enabled":[]}"#,
        r#"{"scores":{"private":3.5},"enabled":[]}"#,
        r#"{"count":"1","label":"duplicate"}"#,
        r#"{"embedding":[1.25,"invalid"]}"#,
        r#"{"identity":"private","unknown":"x"}"#,
    ] {
        assert!(serde_json::from_str::<Record>(json).is_err(), "{json}");
    }
}

#[test]
fn unknown_wire_fields_are_rejected_without_storage() {
    let options = DecodeOptions::new().with_unknown_field_limit(0);
    for wire in [
        &[0xa0, 6, 1][..],
        &[0xab, 6, 8, 1, 0xac, 6][..],
        &[0x5a, 3, 0xa0, 6, 1][..],
    ] {
        assert!(matches!(
            options.decode_from_slice::<Record>(wire),
            Err(DecodeError::UnknownFieldLimitExceeded)
        ));
        assert!(options.decode_view::<RecordView<'_>>(wire).is_err());
        // Lazy children are validated when materialized; no accepted owned
        // result may bypass the shared zero allowance.
        match options.decode_lazy_view::<RecordLazyView<'_>>(wire) {
            Err(DecodeError::UnknownFieldLimitExceeded) => {}
            Ok(view) => assert!(view.to_owned_message().is_err()),
            other => panic!("unexpected result: {other:?}"),
        }
    }
}

#[test]
fn discarded_unknowns_do_not_exhaust_positive_storage_budgets() {
    let wire = [0xa0, 6, 1, 0xa0, 6, 2];
    let options = DecodeOptions::new().with_unknown_field_limit(1);
    assert!(options.decode_from_slice::<Record>(&wire).is_ok());
    assert!(options.decode_view::<RecordView<'_>>(&wire).is_ok());
    assert!(options
        .decode_lazy_view::<RecordLazyView<'_>>(&wire)
        .is_ok());

    // Preserve the default acceptance of discarded unknowns beyond the storage
    // cap. The cap bounds records, which these messages do not materialize.
    let count = buffa::DEFAULT_UNKNOWN_FIELD_LIMIT.checked_add(1).unwrap();
    let wire = [0xa0, 6, 1].repeat(count);
    assert!(Record::decode_from_slice(&wire).is_ok());
    assert!(DecodeOptions::new()
        .reject_unknown_fields()
        .decode_from_slice::<Record>(&wire)
        .is_err());
}

#[test]
fn views_preserve_wire_and_json_behavior() {
    let value = record();
    let wire = value.encode_to_bytes();
    let view = RecordView::decode_view(&wire).unwrap();
    let lazy = RecordLazyView::decode_lazy(&wire).unwrap();
    let owned = RecordOwnedView::decode(wire.clone()).unwrap();
    for json in [
        serde_json::to_string(&view).unwrap(),
        serde_json::to_string(&lazy).unwrap(),
        serde_json::to_string(&owned).unwrap(),
    ] {
        assert_eq!(serde_json::from_str::<Record>(&json).unwrap(), value);
    }
}

#[test]
fn cleanup_builders_avoid_owned_struct_updates() {
    let value = Record::default()
        .with_handle(42)
        .with_index(7)
        .with_identity("private".to_owned())
        .with_key(b"secret".to_vec())
        .with_embedding(vec![1.25, 2.5])
        .with_child(Record::default().with_handle(9));
    assert_eq!(value.handle, 42);
    assert_eq!(value.child.handle, 9);
    assert_eq!(&*value.embedding, &[1.25, 2.5]);
    assert!(!format!("{value:?}").contains("42"));
    let null: Record = serde_json::from_str(r#"{"statusHistory":["STATUS_VALID",null]}"#).unwrap();
    assert_eq!(null.status_history.len(), 2);
    assert_eq!(null.status_history[1].to_i32(), 0);
    assert!(serde_json::from_str::<Record>(
        r#"{"statusHistory":["STATUS_VALID",9223372036854775807]}"#,
    )
    .is_err());
}

#[test]
fn guarded_debug_does_not_reveal_oneof_presence_or_selected_case() {
    let mut expected = None;
    let mut expected_oneof = None;
    for source in [r#"{}"#, r#"{"count":"7"}"#, r#"{"label":"private"}"#] {
        let value: Record = serde_json::from_str(source).unwrap();
        let wire = value.encode_to_bytes();
        let view = RecordView::decode_view(&wire).unwrap();
        let lazy = RecordLazyView::decode_lazy(&wire).unwrap();
        let owned = RecordOwnedView::decode(wire.clone()).unwrap();
        let debug = [
            format!("{value:?}"),
            format!("{view:?}"),
            format!("{lazy:?}"),
            format!("{owned:?}"),
        ];
        if let Some(expected) = &expected {
            assert_eq!(&debug, expected);
        } else {
            expected = Some(debug);
        }
        if let Some(oneof) = &value.secret {
            let debug = format!("{oneof:?}");
            if let Some(expected) = &expected_oneof {
                assert_eq!(&debug, expected);
            } else {
                expected_oneof = Some(debug);
            }
        }
    }
}

#[test]
fn text_roundtrip_streams_into_guarded_owners() {
    let value = record();
    let text = buffa::text::encode_to_string(&value);
    let decoded: Record = buffa::text::decode_from_str(&text).unwrap();
    assert_eq!(decoded, value);
    assert!(buffa::text::decode_from_str::<Record>("embedding: [1.25, invalid]").is_err());
    assert!(
        buffa::text::decode_from_str::<Record>("scores { key: \"private\" value: invalid }")
            .is_err()
    );
}

#[test]
fn empty_cleanup_messages_deserialize_without_mutable_staging() {
    let value: crate::sensitive_test::Empty = serde_json::from_str("{}").unwrap();
    assert_eq!(value, crate::sensitive_test::Empty::default());
    assert!(serde_json::from_str::<crate::sensitive_test::Empty>(r#"{"unknown":42}"#).is_err());
}

#[test]
fn view_cleanup_wipes_owned_slots_and_preserves_borrowed_input() {
    let value = record();
    let wire = value.encode_to_vec();
    let before = wire.clone();
    let mut view = RecordView::decode_view(&wire).unwrap();
    view.zeroize();
    assert_eq!(view.handle, 0);
    assert_eq!(view.score, 0.0);
    assert!(view.embedding.is_empty());
    assert!(view.scores.is_empty());
    assert!(view.created_at.is_unset());
    assert!(view.secret.is_none());
    assert_eq!(wire, before);
    let mut lazy = RecordLazyView::decode_lazy(&wire).unwrap();
    lazy.zeroize();
    assert_eq!(lazy.handle, 0);
    assert!(lazy.embedding.is_empty());
    assert!(lazy.created_at.is_unset());
    assert_eq!(wire, before);
}

#[test]
fn cleanup_preserves_derived_json_duplicate_rejection() {
    use crate::sensitive_test::Numeric;
    for json in [
        r#"{"first":1.25,"first":2.5}"#,
        r#"{"first":null,"first":2.5}"#,
        r#"{"second":"42","second":"43"}"#,
    ] {
        assert!(serde_json::from_str::<Numeric>(json).is_err());
    }
    let value: Numeric = serde_json::from_str(r#"{"first":1.25,"second":"42"}"#).unwrap();
    assert_eq!(value.first, 1.25);
    assert_eq!(value.second, 42);
}
