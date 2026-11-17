fn main() {
    // Generate the test message types for fuzzing.
    // Uses the same protos as the conformance suite.
    let protos_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("conformance/protos");

    if !protos_dir
        .join("google/protobuf/test_messages_proto3.proto")
        .exists()
    {
        println!("cargo:warning=conformance/protos/ not populated, fuzz targets will not compile.");
        println!("cargo:warning=Run `task fetch-protos` first.");
        println!("cargo:rustc-cfg=no_protos");
        return;
    }

    // Views disabled: the conformance test messages have recursive types
    // (corecursive field) which produce infinite-size view structs.
    buffa_build::Config::new()
        .files(&[protos_dir.join("google/protobuf/test_messages_proto3.proto")])
        .includes(&[&protos_dir])
        .generate_views(false)
        .generate_json(true)
        .generate_arbitrary(true)
        .compile()
        .expect("failed to compile test_messages_proto3.proto");

    buffa_build::Config::new()
        .files(&[protos_dir.join("google/protobuf/test_messages_proto2.proto")])
        .allow_message_set(true)
        .includes(&[&protos_dir])
        .generate_views(false)
        .generate_json(true)
        .generate_arbitrary(true)
        .compile()
        .expect("failed to compile test_messages_proto2.proto");

    println!("cargo:rerun-if-changed=build.rs");
}
