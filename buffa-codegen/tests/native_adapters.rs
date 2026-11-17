//! Compatibility, fail-closed metadata and downstream compilation checks.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use buffa::{ExtensionSet, Message};
use buffa_codegen::adapters::*;
use buffa_codegen::generated::descriptor::FileDescriptorSet;
use buffa_codegen::{CodeGenConfig, GeneratedFileKind};

const FIXTURE: &str = "native_adapters.proto";
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

fn descriptors() -> &'static FileDescriptorSet {
    static DESCRIPTORS: OnceLock<FileDescriptorSet> = OnceLock::new();
    DESCRIPTORS.get_or_init(|| {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let output = Command::new(std::env::var_os("PROTOC").unwrap_or_else(|| "protoc".into()))
            .arg("--include_imports")
            .arg(format!("--descriptor_set_out={}", temp.path().display()))
            .args(["-I", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")])
            .args([
                "-I",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../buffa-proto-options/protos"),
            ])
            .args([
                "-I",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../buffa-descriptor/protos"),
            ])
            .arg(FIXTURE)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        FileDescriptorSet::decode_from_slice(&std::fs::read(temp.path()).unwrap()).unwrap()
    })
}

fn manifest() -> AdapterConfig {
    AdapterConfig::from_json(include_bytes!("fixtures/native_adapters.json")).unwrap()
}

fn keyword_manifest() -> AdapterConfig {
    AdapterConfig::from_json(
        br#"{
        "version":1,
        "selectors":[{
            "file":"keyword_adapter.rs", "name":"KeywordNative",
            "wire":{"source":".nativecompat.KeywordWire","rust_type":"crate::wire::KeywordWire"},
            "error":{"rust_type":"crate::ContractError","value":"crate::ContractError::Invalid"},
            "variants":[{"source":"type","name":"Type"},{"source":"self","name":"Current"}]
        }]
    }"#,
    )
    .unwrap()
}

#[test]
fn legal_keyword_wire_values_use_portable_identifier_escaping() {
    let files =
        generate_adapters(&descriptors().file, &[FIXTURE.into()], &keyword_manifest()).unwrap();
    assert!(files[0].content.contains("Self::r#type"));
    assert!(files[0].content.contains("Self::self_"));
}

fn reject(change: impl FnOnce(&mut AdapterConfig), expected: ErrorReason) {
    let mut config = manifest();
    change(&mut config);
    let error = generate_adapters(&descriptors().file, &[FIXTURE.into()], &config).unwrap_err();
    assert_eq!(error.reason, expected);
    assert_eq!(
        error.to_string(),
        format!("adapter generation failed: {expected:?}")
    );
}

#[test]
fn schema_options_and_typed_manifest_generate_the_same_24_adapters() {
    let config = manifest();
    assert_eq!(
        (
            config.selectors.len(),
            config.payloads.len(),
            config.borrowed.len()
        ),
        (17, 2, 5)
    );
    let expected = generate_adapters(&descriptors().file, &[FIXTURE.into()], &config).unwrap();
    let mut codegen = CodeGenConfig::default();
    codegen.generate_adapters = true;
    let generated =
        buffa_codegen::generate(&descriptors().file, &[FIXTURE.into()], &codegen).unwrap();
    let actual: Vec<_> = generated
        .iter()
        .filter(|f| f.kind == GeneratedFileKind::Adapter)
        .collect();
    assert_eq!(actual.len(), 24);
    for (expected, actual) in expected.iter().zip(actual) {
        assert_eq!(
            (&expected.name, &expected.content),
            (&actual.name, &actual.content)
        );
        syn::parse_file(&actual.content).unwrap();
    }
    let again = generate_adapters(&descriptors().file, &[FIXTURE.into()], &config).unwrap();
    assert_eq!(
        expected.iter().map(|f| &f.content).collect::<Vec<_>>(),
        again.iter().map(|f| &f.content).collect::<Vec<_>>()
    );
}

#[test]
fn opting_in_does_not_change_portable_generation_or_wire_json_names() {
    let plain = buffa_codegen::generate(
        &descriptors().file,
        &[FIXTURE.into()],
        &CodeGenConfig::default(),
    )
    .unwrap();
    assert!(plain.iter().all(|f| f.kind != GeneratedFileKind::Adapter));
    let mut codegen = CodeGenConfig::default();
    codegen.adapters = Some(manifest());
    let native = buffa_codegen::generate(&descriptors().file, &[FIXTURE.into()], &codegen).unwrap();
    let portable: Vec<_> = native
        .iter()
        .filter(|f| f.kind != GeneratedFileKind::Adapter)
        .collect();
    assert_eq!(plain.len(), portable.len());
    for (plain, actual) in plain.iter().zip(portable) {
        assert_eq!(
            (&plain.name, &plain.content),
            (&actual.name, &actual.content)
        );
        assert!(!actual.content.contains("include!(\"signing_input.rs\")"));
    }
}

#[test]
fn malformed_and_oversized_manifests_have_fixed_errors() {
    for bytes in [
        b"{}".as_slice(),
        b"{\"version\":1,\"version\":1}".as_slice(),
        b"{\"version\":1,\"unknown\":true}".as_slice(),
        b"{\"version\":1,\"selectors\":null}".as_slice(),
    ] {
        assert_eq!(
            AdapterConfig::from_json(bytes).unwrap_err().reason,
            ErrorReason::InvalidManifest
        );
    }
    assert_eq!(
        AdapterConfig::from_json(b"{\"version\":2}")
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedVersion
    );
    let nested = format!(
        "{{\"version\":1,\"borrowed\":{}0{}}}",
        "[".repeat(256),
        "]".repeat(256)
    );
    assert_eq!(
        AdapterConfig::from_json(nested.as_bytes())
            .unwrap_err()
            .reason,
        ErrorReason::InvalidManifest
    );
    assert_eq!(
        AdapterConfig::from_json(&vec![b' '; MAX_MANIFEST_BYTES + 1])
            .unwrap_err()
            .reason,
        ErrorReason::ManifestTooLarge
    );
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), vec![b' '; MAX_MANIFEST_BYTES + 1]).unwrap();
    assert_eq!(
        AdapterConfig::read_json(file.path()).unwrap_err().reason,
        ErrorReason::ManifestTooLarge
    );
    assert_eq!(
        AdapterConfig::read_json(file.path().join("absent"))
            .unwrap_err()
            .reason,
        ErrorReason::ManifestIo
    );
}

#[test]
fn selector_metadata_cannot_silently_drop_or_reinterpret_cases() {
    reject(
        |c| {
            c.selectors[0].variants.pop();
        },
        ErrorReason::UncoveredCase,
    );
    reject(
        |c| {
            let variant = c.selectors[0].variants[0].clone();
            c.selectors[0].variants.push(variant);
        },
        ErrorReason::DuplicateName,
    );
    reject(
        |c| {
            let variant = c.selectors[0].variants[0].clone();
            c.selectors[0].normalizations.push(Normalization {
                source: variant.source,
                target: variant.name,
            });
        },
        ErrorReason::DuplicateCase,
    );
    reject(
        |c| {
            c.selectors[0].normalizations[0].target = "Absent".into();
        },
        ErrorReason::InvalidNormalization,
    );
    reject(
        |c| {
            c.selectors[0].variants[0].source = "COSE_SIGNATURE_ALGORITHM_UNSPECIFIED".into();
        },
        ErrorReason::UnspecifiedCase,
    );
    reject(
        |c| {
            c.selectors[0].variants[0].source = "NOT_A_SCHEMA_CASE".into();
        },
        ErrorReason::UnknownCase,
    );
    reject(
        |c| {
            c.selectors[0].variants[1].identifier = c.selectors[0].variants[0].identifier;
        },
        ErrorReason::InvalidIdentifiers,
    );
    reject(
        |c| {
            c.selectors[0].variants[0].identifier = None;
        },
        ErrorReason::InvalidIdentifiers,
    );
    reject(
        |c| {
            c.selectors[0].identifier_error = None;
        },
        ErrorReason::InvalidIdentifiers,
    );
    reject(
        |c| {
            c.selectors
                .iter_mut()
                .find(|s| !s.legacy.is_empty())
                .unwrap()
                .legacy[0]
                .normalizations
                .pop();
        },
        ErrorReason::UncoveredCase,
    );
    reject(
        |c| {
            c.selectors[0].wire.source = ".absent.Enum".into();
        },
        ErrorReason::MissingDescriptor,
    );
}

#[test]
fn native_names_paths_and_outputs_are_validated_without_source_injection() {
    reject(
        |c| c.selectors[0].name = "(".repeat(4096),
        ErrorReason::InvalidIdentifier,
    );
    reject(
        |c| c.selectors[0].wire.rust_type = format!("{}u8{}", "T<".repeat(4096), ">".repeat(4096)),
        ErrorReason::InvalidRustPath,
    );
    reject(
        |c| c.selectors[0].file = "mod.rs".into(),
        ErrorReason::InvalidOutput,
    );
    reject(
        |c| {
            c.selectors[0].file = format!(
                "{}.rs",
                c.selectors[1]
                    .file
                    .strip_suffix(".rs")
                    .unwrap()
                    .to_ascii_uppercase()
            )
        },
        ErrorReason::DuplicateOutput,
    );
    reject(
        |c| c.selectors[0].name = "self".into(),
        ErrorReason::InvalidIdentifier,
    );
    reject(
        |c| c.selectors[0].file = "../escape.rs".into(),
        ErrorReason::InvalidOutput,
    );
    reject(
        |c| c.selectors[0].file = "a/b.rs".into(),
        ErrorReason::InvalidOutput,
    );
    reject(
        |c| c.selectors[0].file = c.selectors[1].file.clone(),
        ErrorReason::DuplicateOutput,
    );
    reject(
        |c| c.selectors[0].wire.rust_type = "crate::Wire<u8>".into(),
        ErrorReason::InvalidRustPath,
    );
    reject(
        |c| c.selectors[0].error.value = "{ unreachable!() }".into(),
        ErrorReason::InvalidRustPath,
    );
    reject(
        |c| c.selectors[0].variants[0].aliases.push("ALL".into()),
        ErrorReason::DuplicateName,
    );
    reject(
        |c| c.ports.push(c.ports[0].clone()),
        ErrorReason::DuplicateName,
    );
    let error = generate_adapters(&descriptors().file, &[], &manifest()).unwrap_err();
    assert_eq!(error.reason, ErrorReason::GenerationScope);
}

#[test]
fn payloads_and_borrows_reject_incomplete_or_incompatible_metadata() {
    reject(
        |c| {
            let Payload::Tuple { field } = &mut c.borrowed[0].variants[1].payload else {
                panic!()
            };
            field.source.path = vec!["nested".into(); 65];
        },
        ErrorReason::InvalidPayload,
    );
    reject(
        |c| {
            c.payloads[0].variants.pop();
        },
        ErrorReason::UncoveredCase,
    );
    reject(
        |c| {
            c.borrowed[0].variants.pop();
        },
        ErrorReason::UncoveredCase,
    );
    reject(
        |c| {
            let case = c.borrowed[0].variants[0].cases[0].clone();
            c.borrowed[0].variants[1].cases.push(case);
        },
        ErrorReason::DuplicateCase,
    );
    reject(
        |c| {
            let Payload::Tuple { field } = &mut c.borrowed[0].variants[1].payload else {
                panic!()
            };
            field.source.path = vec!["absent".into()];
        },
        ErrorReason::MissingDescriptor,
    );
    reject(
        |c| {
            let Payload::Tuple { field } = &mut c.payloads[0].variants[0].payload else {
                panic!()
            };
            field.representation = PayloadType::Bytes;
        },
        ErrorReason::OwnershipMismatch,
    );
    reject(
        |c| {
            c.borrowed[3].variants[0].payload = Payload::Port {
                name: "absent".into(),
            }
        },
        ErrorReason::InvalidPayload,
    );
    reject(
        |c| {
            let Payload::Struct { fields } = &mut c.borrowed[4].variants[1].payload else {
                panic!()
            };
            fields.pop();
        },
        ErrorReason::UncoveredField,
    );
    reject(
        |c| {
            let Payload::Struct { fields } = &mut c.borrowed[4].variants[1].payload else {
                panic!()
            };
            fields[1].source.path = vec!["message".into(), "encoded".into()];
        },
        ErrorReason::UnexpectedField,
    );
}

#[test]
fn descriptor_growth_cannot_silently_drop_a_case_or_payload_field() {
    use buffa_codegen::generated::descriptor::field_descriptor_proto::{Label, Type};
    use buffa_codegen::generated::descriptor::{EnumValueDescriptorProto, FieldDescriptorProto};
    let mut files = descriptors().file.clone();
    let file = files
        .iter_mut()
        .find(|f| f.name.as_deref() == Some(FIXTURE))
        .unwrap();
    file.enum_type
        .iter_mut()
        .find(|e| e.name.as_deref() == Some("CoseContentMode"))
        .unwrap()
        .value
        .push(EnumValueDescriptorProto {
            name: Some("COSE_CONTENT_MODE_FUTURE".into()),
            number: Some(99),
            ..Default::default()
        });
    assert_eq!(
        generate_adapters(&files, &[FIXTURE.into()], &manifest())
            .unwrap_err()
            .reason,
        ErrorReason::UncoveredCase
    );

    let mut files = descriptors().file.clone();
    let file = files
        .iter_mut()
        .find(|f| f.name.as_deref() == Some(FIXTURE))
        .unwrap();
    file.message_type
        .iter_mut()
        .find(|m| m.name.as_deref() == Some("CoseEcdsaPrehash"))
        .unwrap()
        .field
        .push(FieldDescriptorProto {
            name: Some("context".into()),
            number: Some(99),
            label: Some(Label::LABEL_OPTIONAL),
            r#type: Some(Type::TYPE_BYTES),
            ..Default::default()
        });
    assert_eq!(
        generate_adapters(&files, &[FIXTURE.into()], &manifest())
            .unwrap_err()
            .reason,
        ErrorReason::UncoveredField
    );
}

#[test]
fn schema_options_fail_closed_on_unknown_or_malformed_metadata() {
    let mut files = descriptors().file.clone();
    let file = files
        .iter_mut()
        .find(|f| f.name.as_deref() == Some(FIXTURE))
        .unwrap();
    let options = file.options.as_option_mut().unwrap();
    let mut native = options.extension(&buffa_proto_options::FILE).unwrap();
    native.adapters.as_option_mut().unwrap().selectors[0]
        .__buffa_unknown_fields
        .push(buffa::UnknownField {
            number: 999,
            data: buffa::UnknownFieldData::Varint(1),
        });
    options.set_extension(&buffa_proto_options::FILE, native);
    let mut config = CodeGenConfig::default();
    // Disabled metadata has no effect, even if a future rule is unrecognized.
    assert!(buffa_codegen::generate(&files, &[FIXTURE.into()], &config).is_ok());
    config.generate_adapters = true;
    assert!(matches!(
        buffa_codegen::generate(&files, &[FIXTURE.into()], &config),
        Err(buffa_codegen::CodeGenError::Adapter(AdapterError {
            reason: ErrorReason::InvalidManifest
        }))
    ));

    let mut mixed = descriptors().file.clone();
    mixed
        .iter_mut()
        .find(|f| f.name.as_deref() == Some(FIXTURE))
        .unwrap()
        .options
        .as_option_mut()
        .unwrap()
        .__buffa_unknown_fields
        .push(buffa::UnknownField {
            number: buffa_proto_options::FILE.number(),
            data: buffa::UnknownFieldData::Varint(1),
        });
    assert!(matches!(
        buffa_codegen::generate(&mixed, &[FIXTURE.into()], &config),
        Err(buffa_codegen::CodeGenError::Adapter(AdapterError {
            reason: ErrorReason::InvalidManifest
        }))
    ));

    let mut files = descriptors().file.clone();
    let options = files
        .iter_mut()
        .find(|f| f.name.as_deref() == Some(FIXTURE))
        .unwrap()
        .options
        .as_option_mut()
        .unwrap();
    options.clear_extension(&buffa_proto_options::FILE);
    options.__buffa_unknown_fields.push(buffa::UnknownField {
        number: buffa_proto_options::FILE.number(),
        data: buffa::UnknownFieldData::LengthDelimited(vec![0xff]),
    });
    assert!(matches!(
        buffa_codegen::generate(&files, &[FIXTURE.into()], &config),
        Err(buffa_codegen::CodeGenError::Adapter(AdapterError {
            reason: ErrorReason::InvalidManifest
        }))
    ));
}

#[test]
fn portable_and_native_output_collisions_are_rejected() {
    let mut config = CodeGenConfig::default();
    let mut native = manifest();
    native.selectors[0].file = "nativecompat.rs".into();
    config.adapters = Some(native);
    config.file_per_package = true;
    assert!(matches!(
        buffa_codegen::generate(&descriptors().file, &[FIXTURE.into()], &config),
        Err(buffa_codegen::CodeGenError::Adapter(AdapterError {
            reason: ErrorReason::DuplicateOutput
        }))
    ));
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn downstream_crate(directory: &Path) -> PathBuf {
    let source_dir = directory.join("src");
    std::fs::create_dir_all(source_dir.join("gen")).unwrap();
    let mut config = CodeGenConfig::default();
    config.generate_adapters = true;
    config.generate_views = false;
    config.generate_json = true;
    config.preserve_unknown_fields = false;
    let mut files =
        buffa_codegen::generate(&descriptors().file, &[FIXTURE.into()], &config).unwrap();
    files.extend(
        generate_adapters(&descriptors().file, &[FIXTURE.into()], &keyword_manifest()).unwrap(),
    );
    let mut native = String::new();
    for file in files {
        std::fs::write(source_dir.join("gen").join(&file.name), &file.content).unwrap();
        if file.kind == GeneratedFileKind::Adapter {
            let stem = file.name.strip_suffix(".rs").unwrap();
            native.push_str(&format!(
                "mod {stem} {{ include!(\"gen/{}\"); }}\npub use {stem}::*;\n",
                file.name
            ));
        }
    }
    let mut code = String::from("#![no_std]\nextern crate alloc;\n#[cfg(test)] extern crate std;\n#[allow(clippy::derivable_impls, clippy::match_single_binding)] pub mod wire { include!(\"gen/nativecompat.mod.rs\"); }\npub mod native {\n");
    code.push_str(&native);
    code.push_str("#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)] pub struct SignatureSuite { pub registration: CoseSignatureAlgorithm, pub curve: CoseCurve }\n}\n");
    let mut coverage = String::from("#[test] fn every_closed_selector_round_trips() {\n");
    for selector in manifest().selectors {
        let wire = selector.wire.source.rsplit('.').next().unwrap();
        coverage.push_str(&format!(
            "round_trip::<{}, wire::{}>({}::ALL);\n",
            selector.name, wire, selector.name
        ));
        if selector.identifier_error.is_some() {
            coverage.push_str(&format!("for &value in {}::ALL {{ assert_eq!({}::from_identifier(value.identifier()), Ok(value)); }}\n", selector.name, selector.name));
        }
    }
    coverage.push_str("}\n");
    code.push_str(
        &include_str!("fixtures/native_adapters_consumer.rs").replace(
            "// Canonical selector coverage is appended by the compatibility harness.",
            &coverage,
        ),
    );
    std::fs::write(source_dir.join("lib.rs"), code).unwrap();
    let root = Path::new(ROOT).canonicalize().unwrap();
    let manifest = directory.join("Cargo.toml");
    std::fs::write(
        &manifest,
        format!(
            r#"[package]
name = "native-adapter-compatibility"
version = "0.0.0"
edition = "2021"
publish = false
[workspace]
[dependencies]
buffa = {{ package = "buffa", path = {runtime:?}, default-features = false, features = ["json"] }}
serde = {{ version = "1", default-features = false, features = ["derive", "alloc"] }}
serde_json = {{ version = "1", default-features = false, features = ["alloc"] }}
"#,
            runtime = root.join("buffa")
        ),
    )
    .unwrap();
    manifest
}

#[test]
fn generated_native_api_compiles_and_runs_without_std() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = downstream_crate(temp.path());
    let target = Path::new(ROOT).join("target/native-adapter-compatibility");
    run(Command::new("cargo")
        .args(["test", "--offline", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target));
    run(Command::new("cargo")
        .args(["clippy", "--offline", "--all-targets", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .args(["--", "-D", "warnings"]));
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .unwrap();
    if String::from_utf8_lossy(&installed.stdout)
        .lines()
        .any(|t| t == "wasm32-unknown-unknown")
    {
        run(Command::new("cargo")
            .args([
                "check",
                "--offline",
                "--target",
                "wasm32-unknown-unknown",
                "--manifest-path",
            ])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target));
    }
}
