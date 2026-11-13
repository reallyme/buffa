# Guarded field representations

An application can keep credentials and personal data in field types that
redact Debug output and wipe their allocations on drop. Use buffa's existing
per-field representation rules; the representation and its security policy
belong in the consuming proto crate.

## Configure generated fields

Mark sensitive fields in the schema:

```protobuf
bytes private_key = 3 [debug_redact = true];
```

The standard option redacts generated owned messages, eager/lazy views, and
oneof variants. It does not change explicit serialization. Public fields such
as a media type can keep their normal representation and Debug behavior.

Configure local representations in `buf.gen.yaml`:

```yaml
plugins:
  - local: protoc-gen-buffa
    out: src/generated
    opt:
      - json=true
      - views=true
      - unknown_fields=false
      - bytes_type_custom_in=.example.Request.private_key=crate::SecretBytes
      - string_type_custom_in=.example.Request.identity=crate::SecretString
```

Both custom-type options are repeatable and use protobuf path prefixes. A
rule can name a field, message, package, or `.` for every field; the last
matching rule wins. Use the field's protobuf path even when it belongs to a
oneof. The equivalent build-script options are `bytes_type_custom_in` and
`string_type_custom_in`.

Custom repeated elements and map slots must use crate-local types because
codegen emits element trait implementations. Do not place one foreign secret
type directly in every proto crate; define a local newtype where needed.

With `strict_utf8_mapping`, maps whose string keys have disabled UTF-8
validation become byte-key maps. Those keys keep ordinary `Vec<u8>` storage;
when their values are bytes too, those values also keep `Vec<u8>` despite
custom representation rules. This existing map shape is outside the guarded
configuration described here. Keep UTF-8 validation enabled for guarded map
keys, and check the generated slot types when selecting representation rules.

## Control binary and JSON allocation

`ProtoBytes::decode` and `ProtoString::decode` let the representation read one
length-delimited field directly from a `bytes::Buf`. Their default methods
preserve the existing `from_wire(WirePayload)` behavior. Override `decode`
when temporary allocations must follow a special ownership policy.

For guarded storage, validate the declared length before allocation, allocate
under a wipe owner before copying, and read chunks directly into that owner.
Validate UTF-8 while byte storage remains guarded. The default decoder may
copy fragmented input into an ordinary `Bytes` before `from_wire` runs; a
`from_wire` implementation alone cannot protect that temporary allocation.

Custom byte representations use their own `serde::Serialize` and
`serde::Deserialize` implementations in every generated JSON field shape:
singular, optional, repeated, map value, and oneof. Those implementations
must preserve protobuf JSON's base64 alphabet/padding rules. Allocate base64
scratch space under a wipe owner before decoding, including when malformed
input can fail after writing partial output. Protobuf null handling remains
at the containing field/container boundary.

The runtime and generator do not depend on a secret-storage library. A local
representation may use `zeroize`, `secrecy`, and its own `base64` dependency.
The test-local implementations in `buffa-test/src/guarded_repr.rs` and their
allocator tests demonstrate the integration; they are not a general-purpose
secure parsing library.

## Lifecycle and ownership boundaries

A field that owns and wipes its allocation protects ordinary assignment,
`Message::clear`, drop, duplicate singular wire fields, oneof switches, and
replacement of a map value. Previously decoded fields and container elements
are dropped on later decoding failures, so their own Drop implementations
perform cleanup. Retain and wipe spare capacity too; converting a secret
`Vec` to a smaller allocation can otherwise release unwiped storage.

Generated messages still implement `Clone`. Each cloned secret field must
become another wipe owner. This preserves buffa's current public trait
contracts; eliminating Clone requires a separate API design and release.

Turning off unknown-field preservation avoids retaining opaque future wire
payloads. Unknown fields are skipped rather than rejected. Applications that
require rejection need an explicit validation policy; an unknown-field count
limit is not a rejection mechanism when preservation is disabled.

Input buffers, borrowed views, `OwnedView`'s reference-counted backing bytes,
reader buffering, serializer output, JSON parser scratch space and parser
errors remain separate ownership boundaries. The application must guard or
otherwise constrain those resources. These hooks do not promise whole-process
zeroization or sanitize a general-purpose parser's diagnostics.

Keep application resource limits after removing source patches:

```rust,ignore
let request: Request = buffa::DecodeOptions::new()
    .with_max_message_size(2 * 1024 * 1024)
    .with_recursion_limit(32)
    .with_element_memory_limit(8 * 1024 * 1024)
    .decode_from_slice(input)?;
```

JSON needs its own input-size and recursion limits. These controls bound
resource usage; wiping controls the lifetime of owned copies.
