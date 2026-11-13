# ReallyMe Buffa

**Protobuf messages with explicit cleanup and export policies for secret-bearing data.**

[![CI](https://github.com/reallyme/buffa/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/reallyme/buffa/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/reallyme-buffa.svg)](https://crates.io/crates/reallyme-buffa)
[![docs.rs](https://img.shields.io/docsrs/reallyme-buffa)](https://docs.rs/reallyme-buffa)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)

[Guarded fields](docs/guarded-fields.md) · [Migration and fork policy](FORK.md) · [Release process](RELEASE.md) · [Release notes](FORK_CHANGELOG.md) · [User guide](docs/guide.md) · [Upstream README](README.upstream.md)

ReallyMe Technologies Limited maintains this fork of [Buffa](https://github.com/anthropics/buffa), a Rust Protocol Buffers implementation with owned messages, borrowed views, protobuf editions, reflection, and `no_std` support. The original upstream README is preserved without edits in [README.upstream.md](README.upstream.md). The [fork overview](docs/overview.md) retains the detailed capabilities, architecture and benchmark information with instructions for this distribution.

## Why this fork exists

ReallyMe uses protobuf messages to carry cryptographic material, personal data, and other sensitive values. Ordinary generated messages can copy those values, print them in `Debug`, or free replaced allocations without wiping them. Addressing those behaviors with patches to generated source makes regeneration and upgrades difficult to maintain.

This fork moves the required mechanisms into the runtime and generator. Consumers can select wipe-owning field representations, control their binary decoding and JSON export, and generate recursive cleanup for complete message families. These choices are explicit and checked during generation, so schema regeneration preserves the configured policy.

The additions are being proposed upstream in [the tracking issue](https://github.com/anthropics/buffa/issues/822) and its linked pull requests. The fork lets ReallyMe use and maintain them while those proposals are reviewed. It is an independent distribution; upstream acceptance and endorsement are not implied.

## What the fork adds

| Area | Capability |
| --- | --- |
| Custom fields | Per-field byte, string and repeated-container configuration through the protoc plugin |
| Binary decoding | Bounded byte/string decode hooks for decoding directly into guarded storage |
| JSON policy | Explicit byte/string policies across owned messages, supported collections and borrowed views |
| Cleanup | Opt-in recursive `Zeroize` and `ZeroizeOnDrop`, Drop-compatible deserialization, and guarded collection/view storage |
| Well-known types | Separate guarded timestamps, durations and dynamic `Struct`/`Value` trees |
| Inspection and export | Borrowed inspection for guarded messages and explicit view JSON restrictions |
| Unknown fields | Wire-field rejection independent of whether unknown fields are retained |

See [guarded fields](docs/guarded-fields.md) for configuration, supported combinations, and the obligations of custom representations.

Cleanup is opt-in. It does not wipe parser scratch, transport bodies, foreign-runtime copies or compiler temporaries outside the owning types. Callers retain their resource limits, canonical decoding checks and transport buffer owners. `Clone` remains available; each guarded clone must own and wipe its own copy.

## Use the ReallyMe packages

The first fork version is **0.1.0**. Registry package names have a `reallyme-` prefix; Rust library names remain unchanged:

```toml
[workspace.dependencies]
buffa = { package = "reallyme-buffa", version = "=0.1.0" }
buffa-types = { package = "reallyme-buffa-types", version = "=0.1.0" }
buffa-build = { package = "reallyme-buffa-build", version = "=0.1.0" }
```

Enable only the features required by the application. Guarded generated messages need `buffa/zeroize`; guarded well-known types need `buffa-types/sensitive`. JSON support remains a separate feature. Preserve `default-features = false` for `no_std` consumers.

Install the matching generator packages:

```sh
cargo install reallyme-protoc-gen-buffa --version '=0.1.0' --locked
cargo install reallyme-protoc-gen-buffa-packaging --version '=0.1.0' --locked
```

Their executable names are `protoc-gen-reallyme-buffa` and `protoc-gen-reallyme-buffa-packaging`. Use these names for Buf's local plugins. Both report version `0.1.0`. Regenerate every schema when switching to this runtime; an upstream generator or hosted SDK is not a substitute for the matching fork build.

The [migration guide](FORK.md#consumer-migration) lists all nine packages, generator checks and Buf configuration. The [logging example](examples/logging/) demonstrates local plugins; the [user guide](docs/guide.md) also covers `build.rs`.

## Upstream relationship and maintenance

The fork has an independent version line and records its exact upstream base in [FORK.md](FORK.md). Version `0.1.0` is based on a development revision after upstream `v0.9.2`, rather than a rebuild of that release.

Upstream updates should arrive through reviewed integration changes, retaining the documented cleanup and export contracts. Contributions suitable for upstream remain in separate PR branches. Published ReallyMe history should remain stable. See the [fork policy](FORK.md) for the sync and release process.

Our preference is to use upstream Buffa. If upstream provides equivalent secret-handling capabilities and guarantees, and our applications can migrate safely, we intend to migrate back upstream and retire active development of this fork. Published ReallyMe versions would remain available for reproducible builds.

## Validation and releases

CI covers formatting, Clippy, default and all-feature tests, strict documentation, code generation freshness, Rust 1.75/1.77, `no_std`, 32-bit compilation and protobuf conformance. Runtime MSRV is Rust 1.75; the opt-in table codec requires Rust 1.77.

Crates.io publication is manually triggered. A release runs the complete CI suite and package preflight before the protected publication job can use a registry credential. Reviewed archives and checksums are retained for inspection. Ordinary pushes and tags do not upload packages.

[RELEASE.md](RELEASE.md) documents the initial token setup, approval step, interrupted-release recovery, and migration to Trusted Publishing. Downloadable generator binaries use a separate manual workflow.

## License and attribution

Upstream Buffa code and ReallyMe modifications are distributed under [Apache-2.0](LICENSE). Existing upstream attribution is retained in the source and preserved upstream README. Modified upstream files identify the ReallyMe changes.

ReallyMe modifications are copyright © 2026 ReallyMe Technologies Limited. This fork does not claim ownership of upstream names or marks, and does not imply endorsement by the upstream maintainers.
