# Native adapters

Buffa can emit process-local Rust enums alongside the portable protobuf model. Generation is opt-in: use `buffa_build::Config::generate_adapters(true)` or the protoc option `adapters=true`. Without either setting, adapter options are ignored and the portable output is unchanged.

Import `buffa/ext/options.proto` from `buffa-proto-options/protos` and declare typed metadata in `(buffa.ext.file).adapters`. The existing `OPTIONS_PROTO` constant includes all the option declarations for build scripts that materialize that file themselves.

```proto
syntax = "proto3";
package example;
import "buffa/ext/options.proto";

enum Mode {
  MODE_UNSPECIFIED = 0;
  MODE_MESSAGE = 1;
  MODE_LEGACY_MESSAGE = 2 [deprecated = true];
}

option (buffa.ext.file).adapters = {
  version: 1
  selectors: {
    file: "mode_adapter.rs"
    name: "MessageMode"
    wire: { source: ".example.Mode" rust_type: "crate::wire::Mode" }
    error: {
      rust_type: "crate::ContractError"
      value: "crate::ErrorReason::InvalidMode"
    }
    variants: { source: "MODE_MESSAGE" name: "Message" }
    normalizations: { source: "MODE_LEGACY_MESSAGE" target: "Message" }
  }
};
```

The generated enum derives `Clone`, `Copy`, `Debug`, `Eq`, `Hash` and `PartialEq`, exposes `ALL`, and implements `TryFrom<buffa::EnumValue<Wire>>` and `From<Native> for Wire`. Zero and unknown numbers return the configured fixed error, using `Into` for the error variant. Every nonzero descriptor value must be a canonical variant, an explicit normalization, or an explicit rejection. New descriptor alternatives therefore require a deliberate adapter decision. Aliases with the same protobuf number cannot map to different native values. Legacy input enums have their own complete mappings; reverse conversion always uses the canonical enum.

`identifier` on a canonical variant is an optional signed external registration. It is independent of the protobuf number. If used, every canonical variant must supply a distinct registration and the selector must supply `identifier_error`; the generator emits `identifier()` and `from_identifier()`. Algorithm policy and provider selection remain application code.

Payload selectors use `NativePayloadSelector` with unit, tuple or struct alternatives. Each payload names an existing schema field and its descriptor type. Value payloads use closed adapters for enums or an application's `Copy` value consisting solely of closed selectors. The Rust compiler also checks `Copy` for every value payload. Owning payload enums cannot contain raw bytes, collections, references or ports. Applications remain responsible for owning and erasing sensitive values; adapter generation does not add cleanup to those values.

Borrowed adapters use `NativeBorrowedAdapter` with either a selector enum or an explicit oneof as their authoritative case set. Byte payloads become `&'a [u8]`. Declared capability ports become `&'a dyn Trait`; they never become wire fields. Borrowed adapters have no generated `Debug`, codec, cloning or cleanup implementation, and never copy or wipe caller buffers. A oneof adapter must cover every alternative and every field in each selected payload, including nested messages. Repeated projections, ambiguous parent/child mappings and ownership changes are rejected.

Outputs are standalone `GeneratedFileKind::Adapter` files with bare ASCII names. `mod.rs` is reserved for packaging, and filename collisions are checked without ASCII case sensitivity. Include them in the native API modules that supply their traits, errors and validated value types. They are deliberately excluded from the protobuf package stitcher and packaging plugin's module tree, so an adapter can retain the same public name as a wire enum. Module gates remain with the API owner:

```rust,ignore
#[cfg(feature = "native-api")]
mod message_mode {
    include!(concat!(env!("OUT_DIR"), "/mode_adapter.rs"));
}
#[cfg(feature = "native-api")]
pub use message_mode::MessageMode;
```

The direct API is `buffa_codegen::adapters::generate_adapters`. Build scripts can supply a typed `AdapterConfig` with `Config::adapters`. A strict JSON representation of that same configuration is available through `AdapterConfig::from_json` / `read_json` and the protoc option `adapter_manifest=path`. JSON is limited to 1 MiB and rejects unknown fields, duplicate fields and excessive nesting. Rust names and paths are parsed as restricted syntax; arbitrary expressions, generic type arguments and source snippets are rejected. Unknown protobuf metadata fields also fail closed when schema adapters are enabled. All adapter failures contain only a fixed `ErrorReason`, never names, paths or input values.

Use a single complete descriptor request (`strategy: all` with Buf) for a schema's adapter bindings. Every authoritative source must be in the requested file set. Imported descriptors remain available for payload-field validation. Explicit configurations replace earlier explicit configurations; if schema options are enabled too, both sets are combined and checked for duplicate native names, port names and output paths before any generated files are returned.

The compatibility fixture under `buffa-codegen/tests/fixtures/native_adapters.proto` covers COSE's 17 closed selectors, two payload selectors and five borrowed adapters. It preserves exact external registrations, explicit legacy normalization and every borrowed matching shape. Its downstream crate runs without `std`, checks lifetime failures and callback ownership, and is checked on `wasm32-unknown-unknown` when that target is installed. Install it with `rustup target add wasm32-unknown-unknown` before running `cargo test -p buffa-codegen --test native_adapters` to include that check.
