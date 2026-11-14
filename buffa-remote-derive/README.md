# reallyme-buffa-remote-derive

**This package is part of ReallyMe Buffa, an independent fork of [upstream Buffa](https://github.com/anthropics/buffa) maintained by ReallyMe Technologies Limited for secret-bearing protobuf messages.**

The fork adds opt-in mechanisms for guarded field storage, zeroization, controlled JSON export, and cleanup of sensitive messages and selected well-known types. We publish this fork on crates.io under separate `reallyme-*` package names so applications can pin and consume these features through Cargo while we maintain the fork and propose the changes upstream.

**Secret handling requires explicit configuration and suitable owning types. It is not automatic.** Transport buffers, parser scratch, and copies outside those owners remain the application's responsibility.

**Compatibility:** this is separately versioned from upstream and is not an unchanged repackaging of Buffa 0.9.2. Use the matching ReallyMe runtime and generator, and regenerate protobuf bindings when migrating.

**Upstream direction:** our preference is to use upstream Buffa. If upstream provides equivalent secret-handling capabilities and guarantees, and our applications can migrate safely, we intend to migrate back upstream and retire active development of this fork. Published ReallyMe versions would remain available for reproducible builds.

This is the **0.1.1** ReallyMe release, distributed under Apache-2.0. Upstream license and attribution notices are retained; upstream endorsement is not implied.

[Configuration and limits](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/docs/guarded-fields.md) · [Migration and provenance](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/FORK.md)

**Package role:** Derive macros for local wrappers around external representation types. The wrapped storage must provide its own cleanup guarantees.

Derive macros for crate-local wrappers around external string, byte and collection representations. The Rust macro path remains `buffa_remote_derive`; these derives implement the storage traits and do not make an external representation wipe its allocations.

```toml
[dependencies]
buffa = { package = "reallyme-buffa", version = "=0.1.1" }
buffa-remote-derive = { package = "reallyme-buffa-remote-derive", version = "=0.1.1" }
```

See the [API reference](https://docs.rs/reallyme-buffa-remote-derive) and [guarded representation requirements](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/docs/guarded-fields.md).
