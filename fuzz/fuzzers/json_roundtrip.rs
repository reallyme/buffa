#![no_main]
#![allow(non_camel_case_types, non_snake_case, dead_code, unused_variables)]

use buffa::Message as _;
use libfuzzer_sys::fuzz_target;

pub use buffa_types::google;

pub mod protobuf_test_messages {
    pub use crate::google;
    pub mod proto3 {
        pub use super::google;
        include!(concat!(
            env!("OUT_DIR"),
            "/protobuf_test_messages.proto3.mod.rs"
        ));
    }
}
pub use protobuf_test_messages::proto3;

fuzz_target!(|data: &[u8]| {
    // Fuzz JSON deserialization. Try to parse the input as UTF-8 JSON,
    // then deserialize as a proto3 message.
    let json = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    let msg: proto3::TestAllTypesProto3 = match serde_json::from_str(json) {
        Ok(m) => m,
        Err(_) => return, // invalid JSON or schema mismatch — expected
    };

    // If JSON decode succeeded, verify binary roundtrip.
    // We do NOT compare encoded bytes directly because HashMap iteration
    // order is non-deterministic (map fields) and NaN != NaN (floats).
    // Instead we verify: encode succeeds, decode of encoded bytes succeeds,
    // and a second encode→decode cycle also succeeds (stability check).
    let encoded = msg.encode_to_vec();

    let computed = msg.compute_size(&mut buffa::SizeCache::new());
    assert_eq!(
        usize::try_from(computed).ok(),
        Some(encoded.len()),
        "compute_size ({computed}) != encode_to_vec len ({})",
        encoded.len()
    );

    let decoded: proto3::TestAllTypesProto3 = buffa::Message::decode_from_slice(&encoded)
        .expect("failed to decode binary from JSON-decoded message");
    let reencoded = decoded.encode_to_vec();

    let redecoded: proto3::TestAllTypesProto3 =
        buffa::Message::decode_from_slice(&reencoded).expect("failed to decode second re-encoding");

    // Structural comparison via JSON: serialize both messages to
    // serde_json::Value (which uses BTreeMap, giving deterministic key
    // order) and compare. This catches semantic differences that byte
    // comparison misses due to HashMap ordering, while also handling
    // NaN correctly (NaN serializes to "NaN" string in proto3 JSON).
    let json1 = serde_json::to_value(&decoded).expect("failed to serialize decoded to JSON");
    let json2 = serde_json::to_value(&redecoded).expect("failed to serialize redecoded to JSON");
    assert_eq!(json1, json2, "binary roundtrip changed message semantics");
});
