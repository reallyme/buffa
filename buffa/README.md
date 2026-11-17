# reallyme-buffa

<!-- Modified by ReallyMe Technologies Limited. -->

**This package is part of ReallyMe Buffa, an independent fork of [upstream Buffa](https://github.com/anthropics/buffa) maintained by ReallyMe Technologies Limited for secret-bearing protobuf messages.**

The fork adds opt-in mechanisms for guarded field storage, zeroization, controlled JSON export, and cleanup of sensitive messages and selected well-known types. We publish this fork on crates.io under separate `reallyme-*` package names so applications can pin and consume these features through Cargo while we maintain the fork and propose the changes upstream.

**Secret handling requires explicit configuration and suitable owning types. It is not automatic.** Transport buffers, parser scratch, and copies outside those owners remain the application's responsibility.

**Compatibility:** this is separately versioned from upstream and is not an unchanged repackaging of Buffa 0.9.2. Use the matching ReallyMe runtime and generator, and regenerate protobuf bindings when migrating.

**Upstream direction:** our preference is to use upstream Buffa. If upstream provides equivalent secret-handling capabilities and guarantees, and our applications can migrate safely, we intend to migrate back upstream and retire active development of this fork. Published ReallyMe versions would remain available for reproducible builds.

This is the **0.1.2** ReallyMe release, distributed under Apache-2.0. Upstream license and attribution notices are retained; upstream endorsement is not implied.

[Configuration and limits](https://github.com/reallyme/buffa/blob/reallyme-v0.1.2/docs/guarded-fields.md) · [Migration and provenance](https://github.com/reallyme/buffa/blob/reallyme-v0.1.2/FORK.md)

**Package role:** The protobuf runtime: messages, wire codecs, decode limits, JSON policy traits and opt-in guarded containers.

[![crates.io](https://img.shields.io/crates/v/reallyme-buffa.svg)](https://crates.io/crates/reallyme-buffa)
[![docs.rs](https://img.shields.io/docsrs/reallyme-buffa)](https://docs.rs/reallyme-buffa)

The runtime crate for **buffa** — a pure-Rust Protocol Buffers implementation
with first-class [editions] support, zero-copy views, and `no_std` compatibility.

This crate contains the `Message` trait, wire-format encode/decode primitives,
`MessageField` / `EnumValue` / `UnknownFields` container types, and the
zero-copy view layer. Generated code emitted by `buffa-build` or
`protoc-gen-reallyme-buffa` depends on this crate.

[editions]: https://protobuf.dev/editions/overview/

## Quick start

See the [workspace README] for end-to-end setup with `buf generate` or
`build.rs`. In short:

```rust,ignore
use buffa::Message;

let bytes = person.encode_to_vec();
let decoded = Person::decode_from_slice(&bytes)?;
```

For untrusted input, tighten limits with `DecodeOptions`:

```rust,ignore
use buffa::DecodeOptions;

let msg: Person = DecodeOptions::new()
    .with_recursion_limit(50)
    .with_max_message_size(1024 * 1024)  // 1 MiB
    .decode_from_slice(&bytes)?;
```

[workspace README]: https://github.com/reallyme/buffa/tree/reallyme-v0.1.2#readme

## Feature flags

| Flag | Default | Enables |
|------|:-------:|---------|
| `std` | ✓ | `std::io::Read` decoders, `HashMap` for map fields, thread-local JSON parse options |
| `fast-utf8` | ✓ | UTF-8 validation of decoded strings with `smoothutf8` in place of `core::str::from_utf8` |
| `json` |  | Proto3 JSON via `serde` |
| `text` |  | Text format (`textproto`) encode/decode |
| `arbitrary` |  | `arbitrary::Arbitrary` impls for fuzzing |
| `zeroize` |  | Guarded containers and generated cleanup support; configure the generator as described in the [guarded-fields guide](https://github.com/reallyme/buffa/blob/reallyme-v0.1.2/docs/guarded-fields.md) |

With `default-features = false` the crate is `#![no_std]` (requires `alloc`).
Disabling the defaults also turns off `fast-utf8`, which does not need `std`; add
`features = ["fast-utf8"]` to keep it.

## Documentation

- API reference: <https://docs.rs/reallyme-buffa>
- User guide: [docs/guide.md]
- Design rationale: [DESIGN.md]

[docs/guide.md]: https://github.com/reallyme/buffa/blob/reallyme-v0.1.2/docs/guide.md
[DESIGN.md]: https://github.com/reallyme/buffa/blob/reallyme-v0.1.2/DESIGN.md

## Conformance

buffa passes the full protobuf [conformance suite] for binary, JSON, and
text-format encoding (both `std` and `no_std` builds).

[conformance suite]: https://github.com/protocolbuffers/protobuf/tree/main/conformance

## License

Apache-2.0
