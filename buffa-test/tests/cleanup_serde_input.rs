//! Cleanup must preserve supported caller deserialization policies.
use buffa_test::cleanup_input::{Guarded, Plain, StrictGuarded, StrictPlain};

#[test]
fn cleanup_preserves_caller_defaults_and_skipped_fields() {
    for input in [
        "{}",
        r#"{"accept":true,"hidden":99,"skippedDefault":99}"#,
        r#"{"accept":{"invalid":[1]},"hidden":[1],"skippedDefault":[]}"#,
    ] {
        let ordinary: Plain = serde_json::from_str(input).unwrap();
        let guarded: Guarded = serde_json::from_str(input).unwrap();
        assert_eq!(ordinary.count, 17);
        assert_eq!(guarded.count, ordinary.count);
        assert_eq!(guarded.accept, ordinary.accept);
        assert!(!guarded.accept);
        assert_eq!(guarded.hidden, ordinary.hidden);
        assert_eq!(guarded.skipped_default, ordinary.skipped_default);
        assert_eq!(guarded.skipped_default, 17);
        assert_eq!(guarded.conditional_default, ordinary.conditional_default);
    }
    for input in [r#"{"count":5}"#, r#"{"count":null}"#] {
        let a: Plain = serde_json::from_str(input).unwrap();
        let b: Guarded = serde_json::from_str(input).unwrap();
        assert_eq!(a.count, b.count);
    }
}

#[test]
fn conditional_input_policies_match_the_derive_and_strict_paths() {
    let input = r#"{"conditional":31}"#;
    let a: Plain = serde_json::from_str(input).unwrap();
    let b: Guarded = serde_json::from_str(input).unwrap();
    assert_eq!(a.conditional, b.conditional);
    assert_eq!(
        b.conditional,
        if cfg!(feature = "arbitrary") { 0 } else { 31 }
    );
    assert_eq!(
        serde_json::from_str::<StrictPlain>(input).is_ok(),
        !cfg!(feature = "arbitrary")
    );
    assert_eq!(
        serde_json::from_str::<StrictGuarded>(input).is_ok(),
        !cfg!(feature = "arbitrary")
    );
    for input in [
        r#"{"accept":true}"#,
        r#"{"hidden":null}"#,
        r#"{"extra":1}"#,
        r#"{"count":1,"count":2}"#,
        r#"{"count":1,"conditionalDefault":[]}"#,
    ] {
        assert!(serde_json::from_str::<StrictPlain>(input).is_err());
        assert!(serde_json::from_str::<StrictGuarded>(input).is_err());
    }
}

#[test]
fn strict_messages_with_every_input_skipped_stay_valid() {
    use buffa_test::cleanup_input::{ConditionalSkippedGuarded, SkippedGuarded};
    assert!(serde_json::from_str::<SkippedGuarded>("{}").is_ok());
    assert!(serde_json::from_str::<SkippedGuarded>(r#"{"value":1}"#).is_err());
    assert!(serde_json::from_str::<ConditionalSkippedGuarded>("{}").is_ok());
    assert_eq!(
        serde_json::from_str::<ConditionalSkippedGuarded>(r#"{"value":1}"#).is_ok(),
        !cfg!(feature = "arbitrary")
    );
}
