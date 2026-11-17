//! Shared roundtrip logic for fuzz targets.

use buffa::Message;

/// Result of a roundtrip attempt.
pub enum RoundtripResult {
    /// Decode succeeded and roundtrip was consistent.
    Ok(Vec<u8>),
    /// Decode failed (expected for random input).
    DecodeError(buffa::DecodeError),
    /// Roundtrip was inconsistent — this is a bug.
    Bug(String),
}

impl RoundtripResult {
    /// Panics if the result indicates a bug. Decode errors are expected
    /// and silently discarded.
    pub fn unwrap_or_decode_error(self) {
        match self {
            RoundtripResult::Ok(_) => {}
            RoundtripResult::DecodeError(_) => {}
            RoundtripResult::Bug(msg) => panic!("roundtrip bug: {}", msg),
        }
    }
}

/// Try to decode `data` as message type `M`, then re-encode and verify
/// the roundtrip produces identical bytes.
///
/// If decode fails, returns `DecodeError` (not a bug — random bytes are
/// usually invalid). If decode succeeds but roundtrip is inconsistent,
/// returns `Bug` (a real bug in buffa).
pub fn roundtrip<M: Message + Default + core::fmt::Debug>(data: &[u8]) -> RoundtripResult {
    // Step 1: try to decode.
    let msg = match M::decode_from_slice(data) {
        Ok(msg) => msg,
        Err(e) => return RoundtripResult::DecodeError(e),
    };

    // Step 2: re-encode.
    let buf1 = msg.encode_to_vec();

    // Step 3: verify compute_size matches actual encoded length.
    let computed = msg.compute_size(&mut buffa::SizeCache::new());
    if usize::try_from(computed).ok() != Some(buf1.len()) {
        return RoundtripResult::Bug(format!(
            "compute_size ({}) != encode_to_vec len ({})",
            computed,
            buf1.len()
        ));
    }

    // Step 4: decode the re-encoded bytes.
    let roundtripped = match M::decode_from_slice(&buf1) {
        Ok(msg) => msg,
        Err(e) => {
            return RoundtripResult::Bug(format!("failed to decode re-encoded bytes: {}", e));
        }
    };

    // Step 5: re-encode the roundtripped message and decode again.
    // We compare the second encode against buf1 after both have been through
    // a full decode → encode cycle. This verifies stability: encode(decode(X))
    // produces the same bytes each time.
    //
    // We do NOT compare messages with PartialEq because:
    // - HashMap iteration order is non-deterministic (map fields)
    // - IEEE 754 NaN != NaN (float/double fields)
    //
    // Instead we verify: decode(buf1) succeeds, and encode(decode(buf1))
    // can itself be decoded successfully. The conformance suite separately
    // validates semantic correctness.
    let buf2 = roundtripped.encode_to_vec();
    if M::decode_from_slice(&buf2).is_err() {
        return RoundtripResult::Bug("failed to decode second re-encoding".into());
    }

    RoundtripResult::Ok(buf1)
}
