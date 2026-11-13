# ReallyMe Buffa

ReallyMe Technologies Limited maintains this Apache-2.0 fork of [the upstream project](https://github.com/anthropics/buffa). It is an independent distribution; no endorsement by the upstream maintainers is implied. The upstream license and existing attribution notices are retained. Modified source files identify the ReallyMe changes.

The first release is **0.1.0**. Its upstream base is `104716922c66e21cf26895a2a5a2a42e6292f9d4`, and its reviewed integration base is `332349be3bd40ae7a33588529ae31f9d8ccba040`. That upstream base is 215 commits beyond upstream tag `v0.9.2`; this is not a rebuild of that tag. The workspace's inherited `0.9.2` manifest version did not describe the compatibility of that later development snapshot.

The fork has an independent version line. Compatible fixes can be released as 0.1.x without waiting for an upstream release; breaking changes use a new minor version. Every release records its upstream base, and all nine packages ship at the same version. Fork release tags use `reallyme-v0.1.0` rather than the upstream tag namespace.

## Packages and Rust names

| Registry package | Dependency alias / Rust library |
| --- | --- |
| `reallyme-buffa` | `buffa` |
| `reallyme-buffa-types` | `buffa-types` / `buffa_types` |
| `reallyme-buffa-descriptor` | `buffa-descriptor` / `buffa_descriptor` |
| `reallyme-buffa-proto-options` | `buffa-proto-options` / `buffa_proto_options` |
| `reallyme-buffa-codegen` | `buffa-codegen` / `buffa_codegen` |
| `reallyme-buffa-build` | `buffa-build` / `buffa_build` |
| `reallyme-buffa-remote-derive` | `buffa-remote-derive` / `buffa_remote_derive` |
| `reallyme-protoc-gen-buffa` | binary `protoc-gen-reallyme-buffa` |
| `reallyme-protoc-gen-buffa-packaging` | binary `protoc-gen-reallyme-buffa-packaging` |

The source directories and protobuf package names are unchanged. Generated Rust still refers to `::buffa`, `::buffa_types` and `::buffa_descriptor`. All published internal dependencies select the ReallyMe packages at exactly 0.1.0. The test, YAML and reference example crates remain unpublished.

## Consumer migration

Change the workspace dependencies and regenerate every schema with this generator release:

```toml
[workspace.dependencies]
buffa = { package = "reallyme-buffa", version = "=0.1.0" }
buffa-types = { package = "reallyme-buffa-types", version = "=0.1.0" }
buffa-descriptor = { package = "reallyme-buffa-descriptor", version = "=0.1.0" }
buffa-proto-options = { package = "reallyme-buffa-proto-options", version = "=0.1.0" }
buffa-build = { package = "reallyme-buffa-build", version = "=0.1.0" }
buffa-remote-derive = { package = "reallyme-buffa-remote-derive", version = "=0.1.0" }
```

Add only the crates and features that the application uses. For guarded fields, enable `buffa/zeroize`; the guarded WKT family needs `buffa-types/sensitive`. JSON and reflection features remain explicit. Preserve any `default-features = false` needed for `no_std`.

Use the ReallyMe package family consistently across generated proto crates and their consumers. Cargo aliases preserve source imports; they do not make types from the upstream runtime interchangeable with types from this runtime. Check the resolved dependency graph for an accidental second runtime.

Install both tools from the same release:

```sh
cargo install reallyme-protoc-gen-buffa --version '=0.1.0' --locked
cargo install reallyme-protoc-gen-buffa-packaging --version '=0.1.0' --locked
protoc-gen-reallyme-buffa --version
protoc-gen-reallyme-buffa-packaging --version
```

The expected outputs are `protoc-gen-reallyme-buffa 0.1.0` and `protoc-gen-reallyme-buffa-packaging 0.1.0`. A generation guard should require these exact names and versions. The binaries can coexist with the upstream binaries.

```yaml
version: v2
plugins:
  - local: protoc-gen-reallyme-buffa
    out: src/generated
    opt:
      - views=true
  - local: protoc-gen-reallyme-buffa-packaging
    out: src/generated
    strategy: all
```

For protoc directly, use `--reallyme-buffa_out` and `--reallyme-buffa-packaging_out`. For build scripts, retain `buffa_build::Config` through the dependency alias above. Current generated views carry lifetime-safety implementations absent from released upstream 0.9.2 output; existing generated files must not be carried across the runtime switch without regeneration.

The historical BSR quickstart remains an upstream-plugin example and is excluded from the fork workspace. For this release, use the local plugins above. No fork BSR plugin is published by this release process.

## Cleanup and export policy

The fork adds bounded custom-field decode hooks, JSON policies across owned and borrowed representations, recursive message cleanup, guarded collections, borrowed inspection, guarded arbitrary generation, explicit view export restrictions, and guarded temporal/dynamic WKTs. Configuration and supported combinations are described in [the guarded-fields guide](docs/guarded-fields.md).

The cleanup profile is opt-in. Representation types must own and wipe their allocations. Both eager and lazy view-to-owned conversion construct a cleanup owner before evaluating fallible fields when that profile is enabled. Borrowed views can encode existing key/plaintext buffers without an intermediate owned message; the encoded output still needs an appropriate cleanup owner.

Keep canonical wire checks, decode size/depth limits, parser and transport buffer ownership, and domain wrappers. A zero unknown-field allowance now rejects discarded unknown fields too. Lazy children are validated when accessed; use an owned or eager path when the entire tree must be accepted at one boundary. Parser scratch, transport copies, compiler temporaries and managed-runtime copies are outside the cleanup guarantee.

## Release procedure

The first publication requires approval of the reviewed commit, package artifacts and publishing identity. Do not push the release branch, create the tag, dispatch a publishing workflow, or upload crates until each corresponding operation is approved. Configure ownership through the approved crates.io account and, where appropriate, a ReallyMe GitHub team. Never place registry tokens in source files or chat.

Validation includes formatting, strict Clippy, tests, regeneration and freshness, test-retention checks, Rust 1.75/1.77, host/ARM `no_std`, 32-bit compilation, native conformance, and unpacked-package consumer checks. Use fresh target directories for each standalone package check and a distinct target directory for the release workspace. Verify renamed dependencies from packaged artifacts, not only workspace paths.

The publication order is checked by `scripts/check-publish-coverage.sh`; `.github/publish-order.txt` is the authoritative list. Cargo 1.95.0 can stage several selected workspace packages during packaging and dry-run publication, so unpublished dependencies can be verified together before the first upload. Run these checks with registry access; offline multi-package verification fails with an internal checksum error in this Cargo version. The dry run reads registry metadata and never uploads packages.

Release and registry workflows are manual. A branch push or tag push does not publish packages or plugin binaries. The initial crate creation needs a token from the approved publishing account; trusted publishing can be configured once the packages exist. Preserve the package checksums and the final Git revision with the release record.

Fork release notes are in [FORK_CHANGELOG.md](FORK_CHANGELOG.md). Historical upstream notes and fragments remain unchanged in `CHANGELOG.md` and `.changes/`; the fork's active changelog sources are under `.changes/reallyme/`.
