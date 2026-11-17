#![no_main]
#![allow(non_camel_case_types, non_snake_case, dead_code, unused_variables)]

use libfuzzer_sys::fuzz_target;

pub use buffa_types::google;

pub mod protobuf_test_messages {
    pub use crate::google;
    pub mod proto2 {
        pub use super::google;
        include!(concat!(
            env!("OUT_DIR"),
            "/protobuf_test_messages.proto2.mod.rs"
        ));
    }
}
pub use protobuf_test_messages::proto2;

fuzz_target!(|data: &[u8]| {
    buffa_fuzz::roundtrip::<proto2::TestAllTypesProto2>(data).unwrap_or_decode_error();
});
