//! Exercise real generated code: source assertions alone cannot establish that
//! nested, map and oneof fields keep their guarded representation.

use crate::guarded::{
    credentials::Credential, Credentials, CredentialsLazyView, CredentialsOwnedView,
    CredentialsView,
};
use crate::guarded_repr::{SecretBytes, SecretString};
use buffa::{bytes::Buf, DecodeError, DecodeOptions, LazyMessageView, Message, MessageView};
use zeroize::Zeroize;

fn fixture() -> Credentials {
    let mut value = Credentials {
        key: b"private-key".to_vec().into(),
        identity: "identity-secret".to_owned().into(),
        token: Some(b"token".to_vec().into()),
        chain: vec![b"certificate".to_vec().into()],
        labels: vec!["label-secret".to_owned().into()],
        child: Some(Credentials {
            key: b"child-secret".to_vec().into(),
            ..Default::default()
        })
        .into(),
        children: vec![Credentials {
            identity: "nested-secret".to_owned().into(),
            ..Default::default()
        }],
        credential: Some(Credential::Raw(b"oneof-secret".to_vec().into())),
        ..Default::default()
    };
    value
        .keys
        .insert("key-name".to_owned().into(), b"map-secret".to_vec().into());
    value
        .identities
        .insert("name".to_owned().into(), "map-identity".to_owned().into());
    value.members.insert(
        "member".to_owned().into(),
        Credentials {
            key: b"member-secret".to_vec().into(),
            ..Default::default()
        },
    );
    value
}

#[test]
fn owned_binary_and_json_roundtrip() {
    let value = fixture();
    let wire = value.encode_to_vec();
    assert_eq!(Credentials::decode_from_slice(&wire).unwrap(), value);
    let json = serde_json::to_string(&value).unwrap();
    assert!(json.contains("cHJpdmF0ZS1rZXk="));
    assert_eq!(serde_json::from_str::<Credentials>(&json).unwrap(), value);
    let (first, rest) = wire.split_at(3);
    assert_eq!(Credentials::decode(&mut first.chain(rest)).unwrap(), value);
}

#[test]
fn clear_and_zeroize_reset_every_owned_shape() {
    let mut value = fixture();
    value.clear();
    assert_eq!(value, Credentials::default());
    let mut bytes = SecretBytes::from(b"secret".to_vec());
    bytes.zeroize();
    assert!(bytes.is_empty());
    let mut text = SecretString::from("secret".to_owned());
    text.zeroize();
    assert!(text.is_empty());
}

#[test]
fn singular_optional_oneof_and_map_replacements_preserve_wire_semantics() {
    let mut value = fixture();
    let replacement = Credentials {
        key: b"new".to_vec().into(),
        identity: "new-name".to_owned().into(),
        token: Some(b"new-token".to_vec().into()),
        credential: Some(Credential::Text("new-oneof".to_owned().into())),
        ..Default::default()
    };
    value
        .merge_from_slice(&replacement.encode_to_vec())
        .unwrap();
    assert_eq!(&*value.key, b"new");
    assert_eq!(&*value.identity, "new-name");
    assert_eq!(value.credential, replacement.credential);
    let decoded = serde_json::from_str::<Credentials>(
        r#"{"key":"b2xk","key":"bmV3","keys":{"same":"b2xk","same":"bmV3"}}"#,
    )
    .unwrap();
    assert_eq!(&*decoded.key, b"new");
    assert_eq!(&**decoded.keys.values().next().unwrap(), b"new");
}

#[test]
fn debug_redacts_owned_eager_lazy_oneof_and_owned_view_values() {
    let value = fixture();
    let bytes = value.encode_to_bytes();
    let view = CredentialsView::decode_view(&bytes).unwrap();
    let lazy = CredentialsLazyView::decode_lazy(&bytes).unwrap();
    let owned_view = CredentialsOwnedView::decode(bytes.clone()).unwrap();
    for output in [
        format!("{value:?}"),
        format!("{view:?}"),
        format!("{lazy:?}"),
        format!("{owned_view:?}"),
        format!("{:?}", value.credential),
        format!("{:?}", view.credential),
    ] {
        assert!(output.contains("REDACTED"));
        for secret in [
            "private",
            "identity-secret",
            "label-secret",
            "oneof-secret",
            "map-secret",
        ] {
            assert!(!output.contains(secret), "{output}");
        }
        assert!(!output.contains("112, 114, 105"));
    }
    assert_eq!(view.to_owned_message().unwrap(), value);
    assert_eq!(lazy.to_owned_message().unwrap(), value);
    assert_eq!(owned_view.to_owned_message(), value);
}

#[test]
fn rejects_malformed_and_oversized_binary_input() {
    for wire in [
        &[0x0a, 0x05, 0x01][..],
        &[0x12, 0x02, 0xff, 0xfe][..],
        &[
            0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02,
        ][..],
    ] {
        assert!(Credentials::decode_from_slice(wire).is_err());
        let (first, rest) = wire.split_at(2);
        assert!(Credentials::decode(&mut first.chain(rest)).is_err());
    }
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    let options = DecodeOptions::new()
        .with_max_message_size(MAX_BYTES)
        .with_recursion_limit(4)
        .with_unknown_field_limit(0);
    assert!(matches!(
        options.decode_from_slice::<Credentials>(&vec![0; MAX_BYTES + 1]),
        Err(DecodeError::MessageTooLarge)
    ));
    let mut value = Credentials::default();
    for _ in 0..8 {
        value = Credentials {
            child: Some(value).into(),
            ..Default::default()
        };
    }
    assert!(matches!(
        options.decode_from_slice::<Credentials>(&value.encode_to_vec()),
        Err(DecodeError::RecursionLimitExceeded)
    ));
}

#[test]
fn unknown_fields_are_skipped_without_retaining_payloads() {
    let wire = [0xa0, 0x06, 0x01];
    let value = Credentials::decode_from_slice(&wire).unwrap();
    assert_eq!(value.encode_to_vec(), Vec::<u8>::new());
    assert_eq!(
        CredentialsView::decode_view(&wire)
            .unwrap()
            .to_owned_message()
            .unwrap(),
        value
    );
    assert_eq!(
        CredentialsLazyView::decode_lazy(&wire)
            .unwrap()
            .to_owned_message()
            .unwrap(),
        value
    );
}

#[test]
fn malformed_json_cleans_up_earlier_fields_and_container_elements() {
    for input in [
        r#"{"key":"c2VjcmV0","identity":[]}"#,
        r#"{"chain":["c2VjcmV0","c2VjcmV0%%%%"]}"#,
        r#"{"labels":["secret",{}]}"#,
        r#"{"keys":{"secret":"c2VjcmV0","other":"%%%%"}}"#,
        r#"{"raw":"c2VjcmV0","text":"duplicate"}"#,
        r#"{"child":{"key":"c2VjcmV0","identity":[]}}"#,
    ] {
        assert!(
            serde_json::from_str::<Credentials>(input).is_err(),
            "{input}"
        );
    }
    for input in [
        r#"{"key":null,"token":null,"chain":null}"#,
        r#"{"key":"-_8"}"#,
        r#"{"key":"+/8="}"#,
    ] {
        assert!(
            serde_json::from_str::<Credentials>(input).is_ok(),
            "{input}"
        );
    }
}

#[test]
fn string_map_keys_support_borrowed_lookup_and_removal() {
    let mut value = fixture();
    assert!(value.keys.contains_key("key-name"));
    assert_eq!(&**value.keys.get("key-name").unwrap(), b"map-secret");
    assert_eq!(&*value.keys.remove("key-name").unwrap(), b"map-secret");
    assert!(!value.keys.contains_key("key-name"));
}

#[test]
fn cloning_remains_explicit_and_public_fields_keep_their_representation() {
    let value = fixture();
    let copy = value.clone();
    assert_eq!(value, copy);
    assert_ne!(value.key.as_ptr(), copy.key.as_ptr());
    let _: &String = &value.media_type;
}

#[test]
fn custom_bytes_preserve_protobuf_null_boundaries() {
    let value: Credentials =
        serde_json::from_str(r#"{"key":null,"token":null,"raw":null}"#).unwrap();
    assert!(value.key.is_empty());
    assert!(value.token.is_none());
    assert!(value.credential.is_none());
    for input in [r#"{"chain":[null]}"#, r#"{"keys":{"name":null}}"#] {
        assert!(serde_json::from_str::<Credentials>(input).is_err());
    }
}

#[test]
fn derived_json_visitor_uses_guarded_singular_and_optional_bytes() {
    use crate::guarded::DirectCredentials;
    let value = DirectCredentials {
        key: b"key".to_vec().into(),
        token: Some(b"token".to_vec().into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&value).unwrap();
    assert!(json.contains("a2V5"));
    assert!(json.contains("dG9rZW4="));
    assert_eq!(
        serde_json::from_str::<DirectCredentials>(&json).unwrap(),
        value
    );
    let value: DirectCredentials = serde_json::from_str(r#"{"key":null,"token":null}"#).unwrap();
    assert!(value.key.is_empty());
    assert!(value.token.is_none());
    for input in [r#"{"key":"%%%%"}"#, r#"{"token":"%%%%"}"#] {
        assert!(serde_json::from_str::<DirectCredentials>(input).is_err());
    }
}
