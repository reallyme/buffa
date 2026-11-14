# ReallyMe Buffa changelog

This is the release history of the fork maintained by ReallyMe Technologies Limited.
Upstream release history remains in [CHANGELOG.md](CHANGELOG.md).

Versioning is independent of upstream. Compatible updates increment the patch version; breaking changes increment the minor version while the major version is zero.

## [0.1.1] - 2026-11-14

### Security

- Enum JSON value errors omit the input name or number. This covers generated enum values, enum field values and elements, registered enum extensions, and reflective enum values. The accepted values and lenient parsing behavior are unchanged. Regenerate existing bindings with the matching generator to update standalone enum deserialization.

- Generated eager, lazy, and owned views and view oneofs honor skip_debug by formatting only a redaction marker with the type name. Preserved unknown fields still round-trip. Regenerate existing bindings to replace their old Debug implementations.

[0.1.1]: https://github.com/reallyme/buffa/releases/tag/reallyme-v0.1.1

## [0.1.0] - 2026-11-13

First ReallyMe release. The upstream base is commit `104716922c66e21cf26895a2a5a2a42e6292f9d4`; the reviewed fork integration is `332349be3bd40ae7a33588529ae31f9d8ccba040`. This base is later than upstream tag `v0.9.2`.

### Added

- A fork-specific README with the original upstream README preserved, and a manually approved CI release pipeline that verifies source, full tests and package checksums before uploading.

- Nine packages under the `reallyme-` prefix, with the existing Rust library names and exact dependencies within the package family.
- Separate generator binaries, `protoc-gen-reallyme-buffa` and `protoc-gen-reallyme-buffa-packaging`, which report the fork version. Generated files identify the fork generator and its version.
- Per-field custom bytes, string and repeated representations in the plugin, and opaque bounded decode hooks for bytes and strings.
- Opt-in JSON policies for custom bytes and strings across owned messages, eager and lazy views, owned views, and supported collection positions. Invalid policy bindings fail closed.
- An opt-in message cleanup profile with recursive cleanup, wipe-owning collections and view storage, streaming collection deserialization, borrowed inspection, and depth-bounded arbitrary generation.
- Drop-compatible serde construction, cleanup-aware view-to-owned construction, and a separate guarded temporal/dynamic WKT family behind `buffa-types/sensitive`.
- Unknown wire-field rejection independent of storage, including map entries and closed-enum values. Deferred lazy children are checked when materialized.
- Generic lifecycle, allocation-release, reflection and integration regressions, including the additional tests submitted upstream.

### Changed

- Consumers select the fork packages through Cargo aliases and regenerate all schemas with this exact generator release. Published upstream 0.9.2 bindings are not a compatibility guarantee for this later source base.
- A zero unknown-field allowance rejects discarded unknown fields as well as stored fields. Preserve the positive/default allowance when discarding without rejection is intended. Static extensions and custom descriptor options are unknown fields for this policy.
- Release publication requires manual dispatch from current `main`; successful crate publication is followed by tag and GitHub release creation, following the ReallyMe SSI release sequence. Ordinary branch pushes do not publish packages or plugin binaries.

### Limits

- Cleanup and JSON policies require explicit configuration and suitable representation types. They do not erase parser, transport, foreign-runtime or compiler-generated copies outside their ownership.
- Canonical wire checks, resource limits and transport ownership remain application responsibilities. Supported policy-map combinations and lazy-validation behavior are documented in the guarded-fields guide.

[0.1.0]: https://github.com/reallyme/buffa/releases/tag/reallyme-v0.1.0
