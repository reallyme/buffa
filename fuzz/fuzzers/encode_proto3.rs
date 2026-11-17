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

fuzz_target!(|msg: proto3::TestAllTypesProto3| {
    // Bail early if compute_size indicates the message is non-trivially large.
    //
    // Arbitrary can turn a few input bytes into megabyte backing Vecs/Strings.
    // The encode codepaths themselves are size-independent (every wire-format
    // path is exercised at any payload size), so large inputs add no coverage.
    //
    // The hard constraint is step 5: serde_json::to_value materializes the
    // full message as a Value tree — 10-50× the wire size (every number is a
    // heap-allocated Number, every key a cloned String, Map is BTreeMap). At
    // 1MB encoded we saw ~50-100MB peak alloc per iteration, and under ASan
    // quarantine + size-class fragmentation the RSS never recovers — hit the
    // 2GB rss limit after ~1.6M iterations (Mar 2026, oom-f1a736fc).
    //
    // 64KB encoded → ~3-5MB Value tree × 2 = sustainable for 14hr runs.
    let size = msg.compute_size(&mut buffa::SizeCache::new());
    if size > 65_536 {
        return;
    }
    let encoded = msg.encode_to_vec();

    // Step 2: verify compute_size matches actual length.
    let computed = msg.compute_size(&mut buffa::SizeCache::new());
    assert_eq!(
        usize::try_from(computed).ok(),
        Some(encoded.len()),
        "compute_size ({computed}) != encode_to_vec len ({})",
        encoded.len()
    );

    // Step 3: decode the encoded bytes.
    // Arbitrary can generate deeply nested recursive messages that exceed
    // the decode recursion limit. This is expected — not a bug in buffa.
    let decoded: proto3::TestAllTypesProto3 = match buffa::Message::decode_from_slice(&encoded) {
        Ok(msg) => msg,
        Err(buffa::DecodeError::RecursionLimitExceeded) => return,
        Err(e) => panic!("failed to decode encoded message: {e}"),
    };

    // Step 4: re-encode and verify stability.
    let reencoded = decoded.encode_to_vec();
    let redecoded: proto3::TestAllTypesProto3 =
        buffa::Message::decode_from_slice(&reencoded).expect("failed to decode re-encoded message");

    // Step 5: structural comparison via JSON (handles map ordering and NaN).
    // JSON serialization can fail for messages with out-of-range WKT values
    // (e.g. Timestamp with seconds outside the valid range), which are valid
    // in binary but not representable in proto3 JSON. Skip the comparison
    // in that case — the binary roundtrip above already verified correctness.
    if let (Ok(json1), Ok(json2)) = (
        serde_json::to_value(&decoded),
        serde_json::to_value(&redecoded),
    ) {
        assert_eq!(json1, json2, "encode roundtrip changed message semantics");
    }
});
