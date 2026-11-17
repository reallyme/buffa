# reallyme-buffa-proto-options

<!-- Modified by ReallyMe Technologies Limited. -->

**This package is part of ReallyMe Buffa, an independent fork of [upstream Buffa](https://github.com/anthropics/buffa) maintained by ReallyMe Technologies Limited for secret-bearing protobuf messages.**

The fork adds opt-in mechanisms for guarded field storage, zeroization, controlled JSON export, and cleanup of sensitive messages and selected well-known types. We publish this fork on crates.io under separate `reallyme-*` package names so applications can pin and consume these features through Cargo while we maintain the fork and propose the changes upstream.

**Secret handling requires explicit configuration and suitable owning types. It is not automatic.** Transport buffers, parser scratch, and copies outside those owners remain the application's responsibility.

**Compatibility:** this is separately versioned from upstream and is not an unchanged repackaging of Buffa 0.9.2. Use the matching ReallyMe runtime and generator, and regenerate protobuf bindings when migrating.

**Upstream direction:** our preference is to use upstream Buffa. If upstream provides equivalent secret-handling capabilities and guarantees, and our applications can migrate safely, we intend to migrate back upstream and retire active development of this fork. Published ReallyMe versions would remain available for reproducible builds.

This is the **0.1.1** ReallyMe release, distributed under Apache-2.0. Upstream license and attribution notices are retained; upstream endorsement is not implied.

[Configuration and limits](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/docs/guarded-fields.md) · [Migration and provenance](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/FORK.md)

**Package role:** Custom protobuf option types used by the ReallyMe Buffa generation pipeline.

Rust types for the custom protobuf options of [buffa](https://crates.io/crates/reallyme-buffa).

`buffa/ext/options.proto`, in this crate's `protos/` directory, declares one option for each kind of schema element that buffa generates Rust for: `(buffa.ext.file)`, `(buffa.ext.message)`, `(buffa.ext.field)`, `(buffa.ext.oneof)`, `(buffa.ext.enum)`, and `(buffa.ext.enum_value)`. Each option is a message, and a setting is a field of that message, so an empty option message leaves the generated code unchanged.

This crate provides the option messages and the extension constants (`buffa_proto_options::FIELD` and the rest) that read an option from a descriptor through `buffa::ExtensionSet`. The crate documentation has an example.

The option types use the ReallyMe Buffa runtime.
