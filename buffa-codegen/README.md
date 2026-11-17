# reallyme-buffa-codegen

**This package is part of ReallyMe Buffa, an independent fork of [upstream Buffa](https://github.com/anthropics/buffa) maintained by ReallyMe Technologies Limited for secret-bearing protobuf messages.**

The fork adds opt-in mechanisms for guarded field storage, zeroization, controlled JSON export, and cleanup of sensitive messages and selected well-known types. We publish this fork on crates.io under separate `reallyme-*` package names so applications can pin and consume these features through Cargo while we maintain the fork and propose the changes upstream.

**Secret handling requires explicit configuration and suitable owning types. It is not automatic.** Transport buffers, parser scratch, and copies outside those owners remain the application's responsibility.

**Compatibility:** this is separately versioned from upstream and is not an unchanged repackaging of Buffa 0.9.2. Use the matching ReallyMe runtime and generator, and regenerate protobuf bindings when migrating.

**Upstream direction:** our preference is to use upstream Buffa. If upstream provides equivalent secret-handling capabilities and guarantees, and our applications can migrate safely, we intend to migrate back upstream and retire active development of this fork. Published ReallyMe versions would remain available for reproducible builds.

This is the **0.1.1** ReallyMe release, distributed under Apache-2.0. Upstream license and attribution notices are retained; upstream endorsement is not implied.

[Configuration and limits](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/docs/guarded-fields.md) · [Migration and provenance](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/FORK.md)

**Package role:** The shared code generation engine used by the build helper and protoc plugin.

Generates Rust message, enum, view and codec implementations from protobuf descriptors. Most consumers should use `reallyme-buffa-build` in `build.rs` or the `reallyme-protoc-gen-buffa` CLI, which configure this engine for their respective workflows.

For direct integration:

```toml
[dependencies]
buffa-codegen = { package = "reallyme-buffa-codegen", version = "=0.1.1" }
```

See the [configuration guide](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/docs/guide.md).
