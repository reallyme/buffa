// Modified by ReallyMe Technologies Limited.
//! Static descriptor decoding stores custom options as unknown wire data.
use buffa::{DecodeError, DecodeOptions, Message};
use buffa_descriptor::generated::descriptor::FieldOptions;

#[test]
fn static_custom_options_follow_the_explicit_unknown_rejection_policy() {
    // FieldOptions reserves this extension range for custom options.
    let wire = [0xc0, 0x3e, 1]; // field 1000, varint 1
    let options = FieldOptions::decode_from_slice(&wire).unwrap();
    assert_eq!(options.encode_to_vec(), wire);
    assert_eq!(
        DecodeOptions::new()
            .reject_unknown_fields()
            .decode_from_slice::<FieldOptions>(&wire),
        Err(DecodeError::UnknownFieldLimitExceeded),
    );
}
