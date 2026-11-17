# reallyme-buffa-types

<!-- Modified by ReallyMe Technologies Limited. -->

**This package is part of ReallyMe Buffa, an independent fork of [upstream Buffa](https://github.com/anthropics/buffa) maintained by ReallyMe Technologies Limited for secret-bearing protobuf messages.**

The fork adds opt-in mechanisms for guarded field storage, zeroization, controlled JSON export, and cleanup of sensitive messages and selected well-known types. We publish this fork on crates.io under separate `reallyme-*` package names so applications can pin and consume these features through Cargo while we maintain the fork and propose the changes upstream.

**Secret handling requires explicit configuration and suitable owning types. It is not automatic.** Transport buffers, parser scratch, and copies outside those owners remain the application's responsibility.

**Compatibility:** this is separately versioned from upstream and is not an unchanged repackaging of Buffa 0.9.2. Use the matching ReallyMe runtime and generator, and regenerate protobuf bindings when migrating.

**Upstream direction:** our preference is to use upstream Buffa. If upstream provides equivalent secret-handling capabilities and guarantees, and our applications can migrate safely, we intend to migrate back upstream and retire active development of this fork. Published ReallyMe versions would remain available for reproducible builds.

This is the **0.1.1** ReallyMe release, distributed under Apache-2.0. Upstream license and attribution notices are retained; upstream endorsement is not implied.

[Configuration and limits](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/docs/guarded-fields.md) · [Migration and provenance](https://github.com/reallyme/buffa/blob/reallyme-v0.1.1/FORK.md)

**Package role:** Standard well-known types and the optional guarded timestamp, duration and dynamic-value families.

Rust types for Google's Protocol Buffers well-known types, including
`Timestamp`, `Duration`, `Any`, `FieldMask`, `Struct`, and wrapper types.
The generated message code is checked in, so using this crate does not require
`protoc` or a build script.

## Quick start

Add both the runtime and well-known-types crates:

```sh
cargo add reallyme-buffa@=0.1.1 --rename buffa
cargo add reallyme-buffa-types@=0.1.1 --rename buffa-types
```

Encode and decode a timestamp with buffa's `Message` trait:

```rust
use buffa::Message;
use buffa_types::Timestamp;

let timestamp = Timestamp {
    seconds: 1_700_000_000,
    nanos: 0,
    ..Default::default()
};
let bytes = timestamp.encode_to_vec();
let decoded = Timestamp::decode_from_slice(&bytes).unwrap();
assert_eq!(decoded, timestamp);
```

Common types such as `Timestamp`, `Duration`, `Any`, `FieldMask`, `Struct`,
`Value`, `ListValue`, and `Empty` are re-exported from `buffa_types`. Wrapper
messages are available under `buffa_types::google::protobuf`.

## Features

| Flag | Default | Enables |
|------|:-------:|---------|
| `std` | ✓ | `Timestamp` conversions to and from `std::time::SystemTime`, and `Timestamp::now` |
| `json` |  | Proto3 JSON via `serde` for `Timestamp`, `Duration`, `Any`, `Struct`, `Value`, `ListValue`, `FieldMask`, `Empty` and the wrappers |
| `chrono` |  | Conversions to and from `chrono` time types |
| `jiff` |  | Conversions to and from `jiff` time types |
| `reflect` |  | Runtime reflection for well-known types; also enables `std` |
| `arbitrary` |  | `arbitrary::Arbitrary` implementations |
| `zeroize` |  | Cleanup traits for ordinary well-known types |
| `sensitive` |  | Guarded `Timestamp`, `Duration`, `Struct`, `Value` and `ListValue` representations; enables `zeroize` |

`Api`, `Type`, `Enum`, `SourceContext` and the messages they contain have no
`serde` impls, with or without `json`.

For JSON, enable the feature and add `serde_json` to your crate:

```sh
cargo add reallyme-buffa-types@=0.1.1 --rename buffa-types --features json
cargo add serde_json
```

The crate also supports `no_std` with `alloc`. A `no_std` build needs the
default features of both crates off, because the default `std` feature of
`buffa` applies to the whole build:

```sh
cargo add reallyme-buffa@=0.1.1 --rename buffa --no-default-features
cargo add reallyme-buffa-types@=0.1.1 --rename buffa-types --no-default-features
```

The conversions between `Duration` and `core::time::Duration` do not need
`std`.

See the [crate documentation](https://docs.rs/reallyme-buffa-types) for conversion
details and the complete API.
