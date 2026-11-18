//! A JSON policy governs only the generated serde impls.
//!
//! `DynamicMessage::try_from_message` needs no generated reflection (the
//! `guarded` fixture generates none): it encodes the message and decodes it
//! against a descriptor. `DynamicMessage` JSON then prints a policy field as
//! standard base64 without calling the policy, so a policy that refuses export
//! is not a confidentiality boundary. This pins the documented behaviour.

use std::sync::Arc;

use buffa_descriptor::generated::descriptor::field_descriptor_proto::{Label, Type};
use buffa_descriptor::generated::descriptor::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
};
use buffa_descriptor::reflect::DynamicMessage;
use buffa_descriptor::DescriptorPool;
use buffa_test::guarded::Sealed;

/// `guarded.Sealed` reduced to its `bytes one = 1` field.
fn pool() -> Arc<DescriptorPool> {
    let set = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("guarded.proto".into()),
            package: Some("guarded".into()),
            syntax: Some("proto3".into()),
            message_type: vec![DescriptorProto {
                name: Some("Sealed".into()),
                field: vec![FieldDescriptorProto {
                    name: Some("one".into()),
                    json_name: Some("one".into()),
                    number: Some(1),
                    label: Some(Label::LABEL_OPTIONAL),
                    r#type: Some(Type::TYPE_BYTES),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    Arc::new(DescriptorPool::new(set).expect("valid pool"))
}

#[test]
fn reflective_json_bypasses_a_policy_that_refuses_export() {
    let sealed = Sealed {
        one: b"top-secret".to_vec().into(),
        ..Default::default()
    };
    // The generated serde impl calls the policy, which refuses export.
    let error = serde_json::to_string(&sealed).unwrap_err().to_string();
    assert!(error.contains("sealed field cannot be exported"), "{error}");

    // Reflection never calls it: the value is printed as standard base64.
    let dynamic = DynamicMessage::try_from_message(&sealed, pool()).unwrap();
    assert_eq!(
        serde_json::to_string(&dynamic).unwrap(),
        r#"{"one":"dG9wLXNlY3JldA=="}"#
    );
}

#[test]
fn reflective_json_reaches_cleanup_profile_messages_too() {
    use buffa_test::sensitive_repr::SecretBytes;
    use buffa_test::sensitive_test::{PolicyContainers, FILE_DESCRIPTOR_SET_BYTES};
    // A cleanup-profile message generates borrowed inspection instead of
    // `to_dynamic`, but `try_from_message` needs only its encoding.
    let mut value = PolicyContainers::default();
    value.keys.push(SecretBytes::from(b"top-secret".to_vec()));
    let pool = Arc::new(DescriptorPool::decode(FILE_DESCRIPTOR_SET_BYTES).unwrap());
    let dynamic = DynamicMessage::try_from_message(&value, pool).unwrap();
    assert_eq!(
        serde_json::to_string(&dynamic).unwrap(),
        r#"{"keys":["dG9wLXNlY3JldA=="]}"#
    );
}
