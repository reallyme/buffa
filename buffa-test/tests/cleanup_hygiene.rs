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
