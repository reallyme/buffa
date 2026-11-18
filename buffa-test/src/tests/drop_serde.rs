use zeroize::Zeroize;

impl Drop for crate::drop_defaults::Declared {
    fn drop(&mut self) {
        self.label.zeroize();
        self.count.zeroize();
        self.note.zeroize();
        self.values.zeroize();
    }
}

#[test]
fn serde_defaults_support_drop_and_preserve_declared_defaults() {
    use crate::drop_defaults::Declared;
    let value: Declared = serde_json::from_str("{}").unwrap();
    assert_eq!(value.label, "configured");
    assert_eq!(value.count, 17);
    assert_eq!(value.note, None);
    let value: Declared =
        serde_json::from_str(r#"{"label":"chosen","count":"42","values":["c2VjcmV0"]}"#).unwrap();
    assert_eq!(value.label, "chosen");
    assert_eq!(value.count, 42);
    assert_eq!(value.values[0], b"secret");
    assert!(serde_json::from_str::<Declared>(r#"{"label":"first","label":"second"}"#).is_err());
    assert!(serde_json::from_str::<Declared>(r#"{"values":["c2VjcmV0","%%%"]}"#).is_err());
}

impl Drop for crate::drop_defaults::CallerDefaults {
    fn drop(&mut self) {
        self.label.zeroize();
        self.values.zeroize();
        self.conditional.zeroize();
    }
}

#[test]
fn caller_serde_defaults_override_generated_defaults_with_drop() {
    let value: crate::drop_defaults::CallerDefaults = serde_json::from_str("{}").unwrap();
    assert_eq!(value.label, "caller-default");
    assert_eq!(value.values, [37]);
    assert_eq!(
        value.conditional,
        if cfg!(feature = "arbitrary") {
            "caller-default"
        } else {
            "schema-fallback"
        }
    );
}
