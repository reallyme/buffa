// Modified by ReallyMe Technologies Limited.
//! A separate consumer crate uses its provider's existing integration traits.
use buffa::{LazyMessageView, Message, MessageView};
use buffa_test::guarded_repr;

pub mod consumer {
    buffa::include_proto!("shared_guarded");
}

#[test]
fn foreign_bytes_need_no_consumer_local_wrapper_or_trait_implementation() {
    let source = r#"{"keys":["c2VjcmV0"],"labels":["private"],"values":{"private":"c2VjcmV0"},"identity":"person"}"#;
    let message: consumer::Credentials = serde_json::from_str(source).unwrap();
    assert_eq!(&*message.keys[0], b"secret");
    let wire = message.encode_to_vec();
    assert_eq!(
        consumer::Credentials::decode_from_slice(&wire).unwrap(),
        message
    );
    for exported in [
        serde_json::to_string(&message).unwrap(),
        serde_json::to_string(&consumer::CredentialsView::decode_view(&wire).unwrap()).unwrap(),
        serde_json::to_string(&consumer::CredentialsLazyView::decode_lazy(&wire).unwrap()).unwrap(),
    ] {
        assert_eq!(
            serde_json::from_str::<consumer::Credentials>(&exported).unwrap(),
            message
        );
    }
}
