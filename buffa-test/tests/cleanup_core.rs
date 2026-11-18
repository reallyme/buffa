//! Cleanup generation is validated without depending on the WKT family.
use buffa::sensitive::{Zeroize, ZeroizeOnDrop};
use buffa::{DecodeOptions, LazyMessageView, Message, MessageView};
use buffa_descriptor::reflect::{InspectMessage, InspectValueRef};
use buffa_test::cleanup_core::{Core, CoreLazyView, CoreView};

fn sample() -> Core {
    serde_json::from_str(r#"{"handle":"37","values":[1.25,2.5],"counts":{"7":"99"},"child":{"handle":"41"},"key":"c2VjcmV0","identity":"private","keys":["c2VjcmV0"],"labels":["one","two"],"material":{"owner":"c2VjcmV0"},"sequence":"17","history":["STATE_ACTIVE"],"states":{"1":"STATE_ACTIVE"},"label":"choice"}"#).unwrap()
}
#[test]
fn owned_eager_lazy_json_text_and_inspection_preserve_values() {
    fn owns_cleanup<T: Zeroize + ZeroizeOnDrop>() {}
    owns_cleanup::<Core>();
    let mut message = sample();
    let wire = message.encode_to_vec();
    let before = wire.clone();
    let mut eager = CoreView::decode_view(&wire).unwrap();
    let mut lazy = CoreLazyView::decode_lazy(&wire).unwrap();
    assert_eq!(eager.to_owned_message().unwrap(), message);
    assert_eq!(lazy.to_owned_message().unwrap(), message);
    for exported in [
        serde_json::to_string(&message).unwrap(),
        serde_json::to_string(&eager).unwrap(),
        serde_json::to_string(&lazy).unwrap(),
    ] {
        assert_eq!(serde_json::from_str::<Core>(&exported).unwrap(), message);
    }
    assert_eq!(
        buffa::text::decode_from_str::<Core>(&buffa::text::encode_to_string(&message)).unwrap(),
        message
    );
    let inspected = message.inspect();
    let identity = inspected.message_descriptor().field(6).unwrap();
    match inspected.get(identity).unwrap() {
        InspectValueRef::String(text) => {
            assert_eq!(text.as_ptr(), message.identity.as_ref().as_ptr())
        }
        _ => panic!("string inspection changed kind"),
    }
    eager.zeroize();
    lazy.zeroize();
    assert_eq!(eager.handle, 0);
    assert!(eager.values.is_empty());
    assert!(lazy.child.is_unset());
    assert_eq!(wire, before);
    message.zeroize();
    assert_eq!(message.handle, 0);
    assert!(message.identity.is_empty());
    assert!(message.counts.is_empty());
    assert!(message.child.is_unset());
    message.clear();
    assert_eq!(message, Core::default());
}
#[test]
fn invalid_input_and_partial_merges_keep_fixed_diagnostics_and_cleanup() {
    for source in [
        r#"{"handle":{"private-token":1}}"#,
        r#"{"material":{"private-token":[]}}"#,
        r#"{"history":["STATE_ACTIVE",null]}"#,
        r#"{"states":{"1":"private-token"}}"#,
    ] {
        let error = serde_json::from_str::<Core>(source)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("private-token"));
    }
    let mut message = sample();
    let options = DecodeOptions::new().clear_on_merge_error();
    assert!(options.merge_from_slice(&mut message, &[0]).is_err());
    assert_eq!(message, Core::default());
    assert!(buffa::text::decode_from_str::<Core>("values: [1.25, bad]").is_err());
    assert!(
        buffa::text::decode_from_str::<Core>("material { key: \"private-token\" value: 99 }")
            .is_err()
    );
}
#[cfg(feature = "arbitrary")]
#[test]
fn arbitrary_construction_propagates_the_depth_bound() {
    use buffa::sensitive::GuardedArbitrary;
    let bytes = [0u8; 512];
    let mut input = arbitrary::Unstructured::new(&bytes);
    assert!(Core::arbitrary_guarded(&mut input, 0).is_err());
    let mut input = arbitrary::Unstructured::new(&bytes);
    assert!(Core::arbitrary_guarded(&mut input, 4).is_ok());
}

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
