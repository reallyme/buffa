# Guarded field representations

<!-- Modified by ReallyMe Technologies Limited. -->

An application can keep credentials and personal data in field types that
redact `Debug` output, wipe their allocations on drop, and control how the
fields are exported. buffa provides the mechanisms; the representation types
and their security policy belong in the consuming proto crate.

## Configure generated fields

Mark sensitive fields in the schema:

```protobuf
bytes private_key = 3 [debug_redact = true];
repeated float embedding = 4 [debug_redact = true];
```

The standard option redacts the field in the `Debug` output of generated owned
messages, eager and lazy views, owned views, and oneof variants. It does not
change serialization. Public fields such as a media type keep their normal
representation and `Debug` behavior.

Select local representations and their JSON policies in `buf.gen.yaml`:

```yaml
plugins:
  - local: protoc-gen-reallyme-buffa
    strategy: all
    out: src/generated
    opt:
      - json=true
      - views=true
      - bytes_type_custom_in=.example.Request.private_key=crate::SecretBytes
      - string_type_custom_in=.example.Request.identity=crate::SecretString
      - repeated_type_custom_in=.example.Request.embedding=crate::SecretList<*>
      - json_policy_type=crate::SecretBytes
      - json_policy_type=crate::SecretString
```

The `*_custom_in` options are repeatable and take protobuf path prefixes. A
rule can name a field, message, package, or `.` for every field; the last
matching rule wins. Use the field's own protobuf path, also when it belongs to
a oneof. The equivalent `buffa-build` methods have the same names.

By default, codegen emits element integration traits for custom representations,
so repeated/map elements must be local to that generating crate. A shared
representation crate can instead provide its `ProtoElemJson`, `ReflectElement`,
`ReflectMapKey` and `InspectElement` impls as needed. Set `custom_type_impls=false`
(builder: `emit_custom_type_impls(false)`) in each consumer to avoid orphan-rule
and duplicate-implementation errors. JSON element impls must delegate to the
representation's selected policy. Missing traits are compile errors. The guarded
WKT byte/string representations provide these integrations themselves.

A `string` rule on a `map` field covers both its key and its value. With
`strict_utf8_mapping`, a map whose string keys have UTF-8 validation disabled
becomes a byte-key map, and its keys keep ordinary `Vec<u8>` storage; when
its values are bytes too, those values also keep `Vec<u8>`. Keep UTF-8
validation enabled for guarded map keys, and check the generated slot types
when you write representation rules.

## Control binary decoding

`ProtoBytes::decode_length_delimited` and
`ProtoString::decode_length_delimited` let a representation read its own field.
buffa reads and validates the field's length prefix, then passes a `FieldPayload`
bounded to exactly that field: `payload.remaining()` is the validated length.
It exposes only bounded `Buf` operations; overrides cannot widen the limit or
access the underlying input. The
defaults pass a `WirePayload` to `from_wire`, which may first copy a field that spans input
chunks into an ordinary `Bytes`. Override the method when that temporary copy
is not acceptable:

```rust,ignore
fn decode_length_delimited(payload: &mut FieldPayload<impl Buf>) -> Result<Self, DecodeError> {
    let mut bytes = Zeroizing::new(Vec::new());
    // Reserve before writing, so no reallocation leaves a copy behind.
    bytes
        .try_reserve_exact(payload.remaining())
        .map_err(|_| DecodeError::MessageTooLarge)?;
    while payload.has_remaining() {
        let chunk = payload.chunk();
        bytes.extend_from_slice(chunk);
        let read = chunk.len();
        payload.advance(read);
    }
    Ok(core::mem::take(&mut *bytes).into())
}
```

Validate UTF-8 while the bytes are still in guarded storage.

## Control JSON

List a custom `bytes` or `string` type with `json_policy_type` and implement
`buffa::json_helpers::ProtoBytesJson` or `ProtoStringJson` for it. Generated
code then sends every JSON use of the type through that implementation:
singular, optional, repeated and oneof fields, `map` keys and values, and the
same fields of generated views, lazy views and owned views. The type's own
`serde` impls are not consulted, and there is no default implementation, so a
listed type without the trait fails to compile.

Serialization receives the field's `&[u8]` or `&str`, which is also all a view
holds, so one policy covers owned and borrowed export. A policy can:

- encode with base64 scratch space it owns and wipes;
- refuse export, by returning an error, while still accepting import;
- decode base64 into storage reserved under a wipe owner before any byte is
  written.

The policy never receives JSON `null`. An implicit-presence field becomes the
default value, an optional field or oneof variant becomes unset, and a `null`
element of a `repeated` field or `map` value is rejected, before the policy
runs. Unlisted custom types keep their existing JSON behavior.

A listed policy type that no unshadowed representation rule selects is a codegen
error. A rule selecting a type with the same final name but a different path
from a listed policy type is also an error; list both paths explicitly when
they intentionally name different types. These checks catch spelling drift,
including a shadowed rule, before it silently uses default JSON behavior.

These checks validate configured bindings, including rules for another directory
in the same project. Prefer Buf's `strategy: all` when sharing cleanup or
view-export selectors so strict selector checks can validate the whole project.
For separate directory requests, explicitly set
`allow_unmatched_message_rules=true`; absent message selectors warn instead of
failing in that request. Validate the complete project once with that option
disabled to catch selector typos. Type spelling and shadowing errors remain
errors in both modes.
Binding and spelling validation also runs with JSON disabled. Valid policy
configuration then warns that its JSON behavior is inactive; invalid bindings
remain errors. Intentional same-name types must each supply and list a policy.

String policy keys are supported for maps with scalar or message values,
except enum-valued maps, whose unknown-enum filtering uses a separate path.
The key policy must emit a JSON string.

Policies apply when a field is serialized. Empty implicit-presence fields are
omitted before their policy runs. This does not provide a blanket prohibition
on serializing a message or view, or remove `Clone` from an owned view. Keep
such consumer restrictions separately, or disable views when they are unused.

To reject unknown JSON fields, use `deny_unknown_json_fields=true`. Its errors
can name the unexpected field, so keep diagnostics from untrusted input out of
logs that must not contain attacker-controlled text.

The default runtime and generator do not require a secret-storage library. A
representation may use `zeroize`, `secrecy` and its own `base64` dependency.
Application byte representations stay in the consuming crate. The optional
guarded WKT family supplies the `SecretString` owner required for its dynamic
tree keys and string values; enabling `buffa-types/sensitive` pulls in its
`secrecy` and `subtle` dependencies. Ordinary WKT users do not enable them.
`buffa-test/src/guarded_repr.rs` and its allocator test show a complete
integration; they are test fixtures, not a general-purpose secure parsing
library.

## Lifecycle and ownership boundaries

A field type that owns and wipes its allocation covers ordinary assignment,
`Message::clear`, drop, duplicate singular wire fields, oneof switches,
replacement of a map value, and fields already decoded when a later part of
the input fails. Keep and wipe spare capacity too: converting a secret `Vec`
into a smaller allocation can release unwiped storage. A custom `repeated`
collection must wipe the allocations it outgrows; `Vec`'s own growth frees the
old buffer without wiping it.

For repeated fields using protobuf scalar or enum JSON helpers, custom
collections receive decoded elements directly through `ProtoList::push`,
including when a later element fails. This covers float embeddings and enum
numbers without staging them in an ordinary `Vec`. A custom collection's
native serde implementation remains responsible for its own temporary storage
where native serde is used.

## Generate recursive cleanup

Enable `buffa/zeroize` and select complete message families with `zeroize=true`
or repeatable `zeroize_in=<protobuf-prefix>` options. The `buffa-build`
equivalents are `generate_zeroize(true)` and `zeroize_in(&[".example"])`.
These messages, and their owned oneof enums, implement `Zeroize`,
`ZeroizeOnDrop` and `Drop`. Numbers, booleans, enum numbers and nested values
are wiped on clear and drop. Generated eager and lazy views also wipe their
owned scalar slots and decoded collection buffers, including outgrown buffers;
borrowed input remains the caller's responsibility. All their fields are redacted in generated
`Debug`, including borrowed views.
Oneof presence and selected cases are redacted as well as their payloads.
Strict JSON unknown-field errors use a fixed diagnostic, and the generated
visitor wipes its owned field-name strings, including rejected names. Other
parser and custom-policy diagnostics remain the caller's error-handling boundary.

The configuration fails closed:

- Reflection generates borrowed inspection instead of an ordinary dynamic
  snapshot. Enable `buffa-descriptor/zeroize` for guarded container bridges.
- `arbitrary=true` uses an early cleanup owner and streams elements directly
  into their destinations, with an explicit depth bound.
- Disable unknown-field storage for these messages (`unknown_fields=false`).
- Every byte and string slot must use a custom `Zeroize + ZeroizeOnDrop` type.
  When JSON is enabled, explicitly list its JSON policy too.
- Every referenced message must implement both cleanup traits. Select child
  message families as well, or supply a guarded imported representation.
  Referenced view types must implement both traits too when views are enabled;
  this protects children that fail before being inserted into their parent.
- Custom collections must implement both traits. With no explicit collection
  rule, codegen selects `buffa::sensitive::List` and `buffa::sensitive::Map`.
  The list wipes outgrown allocations. The ordered map requires `Ord` keys
  and keeps keys and values in stable wipe-owning allocations, so tree
  balancing only moves pointers.

For example, a proto crate can use:

```yaml
opt:
  - json=true
  - deny_unknown_json_fields=true
  - unknown_fields=false
  - zeroize_in=.example
  - bytes_type_custom_in=.example=crate::SecretBytes
  - string_type_custom_in=.example=crate::SecretString
  - json_policy_type=crate::SecretBytes
  - json_policy_type=crate::SecretString
```

List only representation types selected by unshadowed configuration rules. Cleanup
uses the unrolled codec; table selection follows its existing fallback policy.
Keep UTF-8 validation enabled for guarded map keys: strict mapping's raw byte
keys do not support custom key representations.

The checks enforce declared traits and policies, not the correctness of custom
implementations. Each guarded bytes/string type must override the bounded
decode hook to control fragmented-input scratch: implementing `from_wire` and
`ZeroizeOnDrop` alone still permits the default hook's ordinary `Bytes`
temporary. Its JSON policy and `Clone` implementation must also preserve guarded
ownership. Audit these implementations and test fragmented input and failures;
the guarded WKT types and test representations provide examples.

A custom guarded map must also override `MapStorage::storage_erase_key` and
`storage_erase_value`. The decoder uses those hooks to wipe temporary entry
slots when a later field fails or overwrites a value, before insertion gives
the container ownership. `ZeroizeOnDrop` on the container alone cannot protect
such slots, particularly numeric keys and values. The provided guarded `Map`
implements both hooks.

JSON serde is compatible with an application-provided `Drop` implementation
for ordinary generated messages too, and preserves proto2 declared defaults.
The cleanup profile additionally constructs its message owner before parsing
fields, so an error in a later field wipes previously decoded scalar fields.
It preserves duplicate-field rejection for messages that previously used
serde's derived visitor. View-to-owned conversion also constructs the cleanup
owner before materializing children, so a deferred decoding failure wipes
fields already converted.
Map entries and repeated elements enter their guarded destinations directly.
Text decoding streams repeated elements and map entries into these same owners.
Its tokenizer, unescaping buffers and diagnostic storage remain parsing
boundaries, just like JSON parser scratch space.

Rust prohibits struct-update expressions that move owned fields from a value
implementing `Drop`. Construct cleanup messages with mutable field assignment
or their generated `with_<field>` builders. Builders are enabled by default;
keep `with_setters=true` if you customize that option. The normal non-Copy
field moves out of a dropping message or view also require an explicit replacement,
such as `core::mem::take(&mut message.field)`.

## Restrict whole-view JSON export

Use `deny_view_json=.example.Private` (repeatable), or
`buffa_build::Config::deny_view_json(&[".example.Private"])`, when a view must
never serialize even though a policy-approved owned value may serialize.
`.` selects all messages. The restriction covers eager, lazy and owned views,
including empty values, and rejects before writing fields or materializing
lazy children. It propagates through declared message fields, oneofs, maps
and recursive message graphs so containing views cannot partially export data
before encountering a denied child. Rules matching no known message are errors.

Imported descriptors participate in propagation. Apply the rules in each
generation that owns or contains these views: configuration cannot change a
foreign crate's existing serialization implementations or discover that crate's
generation settings. Dynamic `Any` payloads require their own export policy.
Binary encoding, explicit conversion to owned messages and owned-message JSON
are unchanged. This option complements field representation policies.

## Inspect guarded messages

With reflection generation enabled, cleanup messages and their eager views
implement `buffa_descriptor::reflect::InspectMessage`. Import that trait and
call `message.inspect()` to borrow a handle. Enable
`buffa-descriptor/zeroize`; custom collections also provide `InspectList` or
`InspectMap` in their defining crate. The default guarded collections already
provide these bridges. Owned view wrappers expose inspection through `view()`.

This surface has no `to_dynamic` or serde implementation, including on nested
message handles. It never routes a guarded child through the ordinary reflection
bridge. Export uses the owned message's JSON policy. `get` returns a fixed typed
error for foreign descriptors, including an identical descriptor from another
pool, and `has` returns false. Inspection handles and values redact `Debug`.
Returned scalar and numeric-key slots wipe their copies on drop; explicitly
extracting a scalar makes that copy the caller's responsibility. Strings, bytes,
children and collections borrow their original owners. Map lookups borrow their
query keys, including keys returned by inspection. Map deduplication may
allocate an index without copying the field values. Lazy views must materialize
a guarded owned message or be decoded as eager views before inspection.
Erased map lookup may scan O(n) entries. Use `for_each` for full traversal;
it supplies each value directly. Concrete owned `sensitive::Map::get` remains
logarithmic.

Ordinary reflection still creates descriptor-based dynamic snapshots. A custom
JSON policy alone does not select guarded inspection; enable the cleanup profile
for messages that require this contract.

When reflection is enabled, containing messages must join the cleanup profile
too: generation rejects an ordinary container that could snapshot a configured
guarded descendant. Repeat imported families' cleanup selectors in each owning
or containing generation, as with imported view-export policies.

## Construct guarded arbitrary values

With `arbitrary=true`, cleanup messages use generated implementations rather
than deriving through partial aggregates. Enable the consumer's `arbitrary`
feature and `buffa/arbitrary`. Imported guarded WKTs additionally need
`buffa-types/arbitrary`.

Other imported message families must also provide `GuardedArbitrary`; enable
their arbitrary feature or supply the implementation in their defining crate.

The builder creates its cleanup owner first, streams collection elements into
their destinations, and constructs byte/string values from borrowed arbitrary
input through `from_wire`. Their implementations must preserve guarded storage,
as for binary decoding. Map keys remain guarded if a later value fails.
`GuardedArbitrary::arbitrary_guarded` accepts an explicit depth bound; the normal
`Arbitrary` entry point uses Buffa's recursion limit. Each message or oneof
construction consumes a level, so recursive schemas terminate even when values
would otherwise consume no input. Input storage remains the harness's owner.

## Guard well-known types

Enable `buffa-types/sensitive` and map imported types to this separate family:

```yaml
opt:
  - extern_path=.google.protobuf.Timestamp=::buffa_types::sensitive::Timestamp
  - extern_path=.google.protobuf.Duration=::buffa_types::sensitive::Duration
  - extern_path=.google.protobuf.Value=::buffa_types::sensitive::Value
  - extern_path=.google.protobuf.Struct=::buffa_types::sensitive::Struct
  - extern_path=.google.protobuf.ListValue=::buffa_types::sensitive::ListValue
```

These types preserve protobuf wire and JSON forms while owning recursive
cleanup. Dynamic trees use guarded strings, lists and maps; temporal JSON
formatting uses bounded wipe-owning stack storage. Borrowed and owned views
are available; JSON serialization of a view materializes a guarded owned
value. The guarded family supports borrowed inspection with `sensitive,reflect`
and guarded arbitrary construction with `sensitive,arbitrary`. It has no
chrono/jiff adapter implementations.

Ordinary WKTs remain source-compatible and do not gain `Drop`. The lighter
`buffa-types/zeroize` feature adds explicit `Zeroize` implementations for these
five ordinary WKTs; that alone is insufficient for a cleanup-profile child,
which also requires `ZeroizeOnDrop`.

## Reject unknown wire fields

A zero unknown-field allowance applies independently of storage. It
rejects encountered unknown fields, discarded fields inside groups and map
entries, and unknown closed-enum occurrences, even if a later known value
overwrites them. Use `DecodeOptions::reject_unknown_fields()` or
`with_unknown_field_limit(0)` together with `unknown_fields=false`.

Static generated messages treat extension payloads, MessageSet items and custom
descriptor options as unknown fields too, so this rejects them. In contrast,
dynamic messages recognize extensions registered in their descriptor pool.
Do not use a rejecting static descriptor decoder when custom options must be
retained.

Positive allowances retain their storage-budget semantics: discarded fields do
not consume record slots. The default decoder therefore continues accepting
discarded unknowns beyond that budget. Explicit rejection remains active in
deferred children, independently of a positive storage budget reaching zero.

**Constructing a lazy view does not establish rejection throughout the tree.**
Lazy nested messages remain unvalidated until materialized. Decode an owned
message or eager view when validation of the complete tree is required at the
boundary. Canonical re-encoding checks still belong in application codecs:
unknown rejection does not reject duplicate known fields or every noncanonical
encoding.

## Integrate RPC contracts

RPC runtimes and generated service stubs must support the same Buffa API as
the generated messages. In particular, generic code constructing `OwnedView`
must carry `ViewLifetimeParametric` bounds, and owned-view accessors require
`ViewReborrow`. Regenerating message types alone cannot repair missing bounds
inside a transport dependency. Keep these safety requirements when adapting
the transport. A renamed fork must also replace the transport's transitive
Buffa dependency so both use the same `Message` and view traits.

Disabling unknown-field storage requires rejection at the transport decode
boundary. Configure its `DecodeOptions` with zero unknown-field allowance.
Connect 0.9.1 already exposes `RequestContext::with_decode_options` and
`Payload::with_decode_options`; a server interceptor can apply the same policy
to both the context and the unary payload, and to the stream context. These
are public but doc-hidden APIs, so pin the transport version and test the
integration. This application policy does not require a Connect API change.

For paths that first decode with default options, validate the retained
protobuf bytes before domain admission. Connect's `ServiceRequest::bytes`,
stream message bytes and `OwnedView::bytes` retain that input. An already
decoded message with discarded fields cannot recover it. This extra check
does not protect allocations made by the first decode; enforce transport body
limits as well. Eager views validate nested messages; lazy views require
materialization. JSON must be rejected at its original deserializer: bytes
re-encoded from JSON cannot establish what the original JSON contained.

The lifetime-bound compatibility work for a transport using an older Buffa
API is separate from these policies. Keep that adaptation in the transport
integration, without weakening Buffa's lifetime guarantees or making the
guarded APIs depend on it.

Generated views borrow `str` and byte slices even when owned fields use guarded
representations. Keep the RPC body owner and domain conversion guards. A
separate Prost/Tonic generation of the same schemas needs its own cleanup
policy; Buffa options only affect Buffa output. Configure JSON rejection and
resource limits in each protocol path independently.

## Remaining ownership boundaries

Generated messages still implement `Clone`. Each cloned guard must own and
wipe its copy. Input buffers, borrowed views, an owned view's reference-counted
backing bytes, reader buffering, serializer output, JSON parser scratch space
and parser errors remain application ownership boundaries. A guard cannot
clean copies the caller made before passing data to it, or arbitrary compiler,
register and stack copies. These APIs provide best-effort cleanup of the
storage they own, following `zeroize`'s guarantees.

Resource limits bound how much an attacker can make the decoder allocate;
wiping controls how long owned copies live. Use both:

```rust,ignore
let request: Request = buffa::DecodeOptions::new()
    .reject_unknown_fields()
    .with_max_message_size(2 * 1024 * 1024)
    .with_recursion_limit(32)
    .with_element_memory_limit(8 * 1024 * 1024)
    .decode_from_slice(input)?;
```

JSON input needs its own size and recursion limits.

## Build on older Rust toolchains

The workspace permits compatible `zeroize` releases so consumers can share
its traits with newer dependencies. MSRV CI regenerates a compatible lockfile
with modern Cargo's MSRV-aware resolver. Older Cargo can otherwise select a
`zeroize` release requiring newer Rust; before building on Rust 1.75, use
`cargo update -p zeroize --precise 1.8.2` and retain that lockfile.

## Clearing an existing destination after a failed merge

For an existing guarded message, use
`DecodeOptions::new().clear_on_merge_error()` with `merge` or
`merge_from_slice`. A failed merge clears the whole destination, including
old values and fields applied earlier in that input. It also clears on an
input size rejection. A successful merge still follows normal protobuf merge
rules. This option calls the message's own `clear` implementation; it only wipes
memory for fields whose cleanup implementation does so. It does not erase
the caller's input.

## Owning a guarded borrowed view

Use the build option `borrowed_views_only_in(&[".my.pkg.Private"])` or the
repeatable plugin parameter `borrowed_views_only_in=.my.pkg.Private` to omit
standard owned wrappers and the `HasMessageView` impl that exposes them.
Containing messages are covered too, so their standard handles do not become
an alternative way to retain the nested wire bytes. Eager and lazy borrowed
views remain available. Apply the same selectors in every generation step
that can see the relevant schemas. Coverage follows protobuf message fields;
it cannot infer the type inside `Any` or an opaque wire buffer.

Use `buffa::sensitive::GuardedOwnedView::<PrivateView<'static>>` to own an eager
view with private backing storage. Supply the same `DecodeOptions` you use for
admission. `decode` copies from caller-owned storage; `from_owned` encodes
directly into a private wiping allocation. Neither hands out a clonable handle
or a `Bytes` copy or alias. The guarded owner does not implement JSON export. Use guarded generation
to wipe separately allocated values the view itself owns. The message's JSON
and debug policies still apply to the borrowed view.

The generator option removes the automatic owned-view API. Rust code that
deliberately calls the public low-level `OwnedView` constructor with a borrowed
view type can still choose ordinary backing storage. Keep the guarded owner
and admission function at the application boundary.
