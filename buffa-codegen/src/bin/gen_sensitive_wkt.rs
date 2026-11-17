// Modified by ReallyMe Technologies Limited.
//! Regenerate opt-in guarded WKT representations without changing ordinary WKTs.
use buffa_codegen::generated::descriptor::FileDescriptorSet;
use std::{fs, path::PathBuf};

#[derive(Debug, thiserror::Error)]
#[error("guarded WKT generation failed: {self:?}")]
enum ErrorReason {
    Arguments,
    Input,
    DecodeOptions,
    Descriptor,
    Generate,
    Output,
}

fn main() -> Result<(), ErrorReason> {
    let mut arguments = std::env::args_os().skip(1);
    let input = arguments.next().ok_or(ErrorReason::Arguments)?;
    let output = PathBuf::from(arguments.next().ok_or(ErrorReason::Arguments)?);
    if arguments.next().is_some() {
        return Err(ErrorReason::Arguments);
    }
    let bytes = fs::read(input).map_err(|_| ErrorReason::Input)?;
    let descriptors = buffa_codegen::tooling_decode_options()
        .map_err(|_| ErrorReason::DecodeOptions)?
        .decode_from_slice::<FileDescriptorSet>(&bytes)
        .map_err(|_| ErrorReason::Descriptor)?;
    let mut config = buffa_codegen::CodeGenConfig::default();
    config.generate_zeroize = true;
    config.preserve_unknown_fields = false;
    config.generate_views = true;
    config.lazy_views = true;
    config.generate_text = true;
    config.generate_arbitrary = true;
    config.generate_reflection = true;
    config.gate_reflect_on_crate_feature = true;
    config.emit_custom_type_impls = false;
    config.string_fields = vec![(
        ".google.protobuf".into(),
        buffa_codegen::StringRepr::Custom("crate::sensitive::SecretString".into()),
    )];
    config.message_attributes = ["Timestamp", "Duration"]
        .into_iter()
        .map(|name| {
            (
                format!(".google.protobuf.{name}"),
                "#[derive(Eq, Hash)]".into(),
            )
        })
        .collect();
    let files =
        ["timestamp", "duration", "struct"].map(|name| format!("google/protobuf/{name}.proto"));
    let generated = buffa_codegen::generate(&descriptors.file, &files, &config)
        .map_err(|_| ErrorReason::Generate)?;
    fs::create_dir_all(&output).map_err(|_| ErrorReason::Output)?;
    for file in generated {
        fs::write(output.join(file.name), file.content).map_err(|_| ErrorReason::Output)?;
    }
    Ok(())
}
