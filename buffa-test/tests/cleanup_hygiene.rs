#[test]
fn cleanup_json_constructors_are_hygienic_without_views_or_oneofs() {
    use buffa_test::cleanup_json_hygiene::{Empty, Record};
    let value: Record = serde_json::from_str(r#"{"value":17}"#).unwrap();
    assert_eq!(value.value, 17);
    assert!(serde_json::from_str::<Empty>("{}").is_ok());
    for source in [r#"{"value":1,"value":2}"#, r#"{"unknown":1}"#] {
        assert!(serde_json::from_str::<Record>(source).is_err());
        assert!(serde_json::from_str::<Empty>(source).is_err());
    }
}

#[test]
fn cleanup_oneof_setters_preserve_consumer_value_namespaces() {
    use buffa::{LazyMessageView, Message, MessageView, SizeCache, ViewEncode};
    use buffa_test::cleanup_setter_hygiene::{
        record::Selection, Record, RecordLazyView, RecordView,
    };

    assert!(Record::default().encode_to_vec().is_empty());
    let value = Record::default()
        .with_selection(Selection::Number(17))
        .with_optional_number(29);
    assert_eq!(value.selection, Some(Selection::Number(17)));
    assert_eq!(value.optional_number, Some(29));
    let value = value.with_selection(Selection::Number(23));
    assert_eq!(value.selection, Some(Selection::Number(23)));
    let bytes = value.encode_to_vec();
    assert_eq!(bytes, [8, 23, 16, 29]);
    assert_eq!(Record::decode(&mut bytes.as_slice()).unwrap(), value);
    let eager = RecordView::decode_view(&bytes).unwrap();
    assert_eq!(eager.encode_to_vec(), bytes);
    let lazy = RecordLazyView::decode_lazy(&bytes).unwrap();
    let mut cache = SizeCache::new();
    assert_eq!(lazy.compute_size(&mut cache), 4);
    let mut lazy_bytes = Vec::new();
    lazy.write_to(&mut cache, &mut lazy_bytes);
    assert_eq!(lazy_bytes, bytes);
}
