# Wipe-owning well-known types

Enable `buffa-types/sensitive` and select `buffa_types::sensitive` types explicitly
with extern-path mappings. Ordinary well-known types keep their existing public
fields and ownership. The new family uses wiping scalar/string owners and
collections that wipe current and outgrown allocations; generated cleanup owns
its message, oneof and view slots.

JSON support is opt-in. Borrowed eager and lazy serialization keeps sorted
object keys and last-duplicate semantics without constructing an owned tree.
Scratch reference slots are wiped after their borrows end. Errors retain fixed
diagnostics and wire-decode precedence, including partial output behavior.
Caller input and serializer/parser-owned scratch remain with those owners.

This proposal builds on recursive cleanup generation, guarded storage and wire
ownership, explicit ordinary-WKT cleanup, borrowed inspection, and the temporal
writer helpers. Its source and minimum dependencies retain Rust 1.75 support with an
MSRV-compatible lockfile, as documented by the runtime proposals. Modern
consumers may select newer compatible zeroize releases.

For a schema containing a Timestamp field, configure the selected generation:

```rust,ignore
let config = buffa_build::Config::new()
    .files(&["protos/event.proto"])
    .includes(&["protos/"])
    .extern_path(".google.protobuf.Timestamp", "::buffa_types::sensitive::Timestamp")
    .preserve_unknown_fields(false)
    .generate_zeroize(true);
```

Other string/bytes fields and custom containers must also use wipe-owning
representations that satisfy the cleanup generator's checks.

An eager wire owner has a concrete view type:

```rust
use buffa::{DecodeOptions, sensitive::{GuardedOwnedView, GuardedViewError}};
use buffa_types::sensitive::TimestampView;

fn decode_timestamp(input: &[u8]) -> Result<GuardedOwnedView<TimestampView<'static>>, GuardedViewError> {
    GuardedOwnedView::decode(input, &DecodeOptions::new())
}
```

This copies the input into private wiping storage; the caller still owns the
input buffer. `GuardedOwnedView::from_owned` encodes a message directly into
such storage. Ordinary `encode_to_vec`, `encode_to_bytes` and `OwnedView`
outputs keep ordinary ownership. Accessors such as `as_number` return scalar
copies whose cleanup belongs to the caller.
