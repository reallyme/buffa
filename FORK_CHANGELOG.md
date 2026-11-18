# ReallyMe Buffa changelog

This is the release history of the fork maintained by ReallyMe Technologies Limited.
Upstream release history remains in [CHANGELOG.md](CHANGELOG.md).

Versioning is independent of upstream. Compatible updates increment the patch version; breaking changes increment the minor version while the major version is zero.

Release 0.1.3 is an explicit exception to patch compatibility: it raises the MSRV to Rust 1.100, changes external integration lines, and removes ordinary owned-view wrappers for messages that opt into cleanup. Its Breaking changes section records the required migration.

## [0.1.3] - 2026-11-18

### Breaking changes

- Upgrade dependencies, including the public foldhash 0.2, hashbrown 0.17 and jiff 1.0 integrations. Downstream code naming those external types must use these compatibility lines.

- Require Rust 1.100 for every feature, pin development and release tooling to Rust 1.100.0, and verify Cargo lockfile format 4 during package validation.

- Messages selected for cleanup (`generate_zeroize(true)` / `zeroize=true`, or `zeroize_in`), and the messages that contain them in the same generation step, no longer get a generated `FooOwnedView` wrapper or `HasMessageView` impl, as if they were also listed in `borrowed_views_only_in`. The wrapper's `from_owned` encoded the guarded message into ordinary, clonable `Bytes` that were never wiped. This also removes `TimestampOwnedView`, `DurationOwnedView`, `ValueOwnedView`, `StructOwnedView` and `ListValueOwnedView` from `buffa_types::sensitive`. To migrate, replace `FooOwnedView::decode(bytes)` with `buffa::sensitive::GuardedOwnedView::<FooView<'static>>::decode(&bytes, &options)` (or `from_owned(&message, &options)`), which returns `GuardedViewError`, and read fields through `owner.view().field`; the guarded owner is not `Clone`, does not hand out `Bytes`, and implements no JSON itself, so serialize `owner.view()` instead. Code that deliberately accepts unwiped backing storage can still use `buffa::OwnedView::<FooView<'static>>`. Generic code bounded on `HasMessageView`, such as RPC transport glue, no longer accepts these messages. A message whose guarded child is only selected in another generation step, or is an imported guarded type such as `buffa_types::sensitive::Timestamp`, keeps its wrapper unless you also select it with `borrowed_views_only_in`.

### Added

- Add opt-in, descriptor-checked native adapter generation through typed protobuf options, Rust configuration and bounded manifests. Closed selectors reject unspecified and unknown values, preserve explicit normalization and external registrations, and provide checked conversions. Payload selectors and borrowed byte/capability adapters preserve native signatures without changing portable generation.

- `DecodeOptions::clears_on_merge_error` reports whether `clear_on_merge_error` is set. The additive `with_clear_on_merge_error(bool)` setter can enable or disable the policy, including on a cloned configuration. Documentation distinguishes logical clearing, memory wiping, and input-buffer consumption.

- `buffa_codegen::plugin_response_metadata()` returns a `CodeGeneratorResponse` carrying buffa's supported plugin features and the `EDITION_PROTO2` to `EDITION_2024` edition range. Plugins built on buffa-codegen can merge it into each success and error response with `..plugin_response_metadata()`; both generator binaries build their responses from it.

- **`json_helpers::bytes::decode_into` and `bytes::max_decoded_len`** decode a base64 string once into a buffer the caller provides, without allocating and accepting exactly what `bytes::deserialize` accepts, so a `ProtoBytesJson` policy can keep decoded bytes only in storage it wipes. A failure reports a fixed message ("invalid base64" or "base64 output buffer too small") and never quotes a symbol or offset from the input.

### Changed

- Remove unused pbjson-types from the Prost benchmark and stop forcing the optional chrono feature through buffa-test. The buffa-types Chrono conversion API and its explicit-feature test coverage remain available.

- **`DecodeContext` records where the message being decoded ends.** The default `Message::merge_to_limit`, `Message::merge_length_delimited` and the length-delimited decode entry points record it through the new `DecodeContext::with_message_end`, whose argument is the number of bytes that follow the message (the same `limit` that `merge_to_limit` takes), not the message's length. Hand-written code that decodes a different buffer, such as a copy or a decompressed payload, with a context it was given must call `with_message_end(0)` for a buffer that holds exactly the message, or start that buffer with `Message::merge`; otherwise a valid nested field near its end can be rejected with `DecodeError::UnexpectedEof`. A `merge_to_limit` override should record its `limit` the same way. A direct `merge_to_limit` caller can now get `UnexpectedEof` where it previously got `Ok` with the buffer past its limit.

- **`json_helpers::bytes` streams base64 output and decodes URL-safe input once.** `bytes::serialize` writes the encoding through `Serializer::collect_str` instead of building a heap `String` of the encoded length first; the output is unchanged for `serde_json` and for any serializer whose `collect_str` matches its `serialize_str`. Decoding picks the standard or URL-safe alphabet from the first symbol only one of them has and decodes once, where URL-safe input was previously decoded with the standard alphabet first and then decoded again. Accepted inputs and decoded bytes are unchanged; the error text for invalid input that contains `-` or `_` can differ.

- **`buffa-yaml` decodes string and container positions by the kind of the YAML scalar.** Following the wrong-kind diagnostics change, these positions now read the input kind as ProtoJSON does. A YAML number or bool is no longer read as text in an `optional`, repeated, map or oneof string position or as a `Timestamp`, `Duration` or `FieldMask`, matching singular string fields; quote such scalars (`"1.0"`). An empty item or map value (`-` followed by a space, or `key:`) is now null, so it is rejected as a repeated string, a `FieldMask`, `Struct`, `ListValue` or `Empty`, or a message with a custom `Deserialize` (a oneof or extension range) in an element or map-value position, where it previously decoded as an empty value. An empty or comment-only YAML document still decodes as the default message, `Struct`, `ListValue` or `Empty`, through generated types and through `DynamicMessageSeed`. Explicit empty documents (`---`) are rejected at structural roots.

- Execute guarded cleanup regression tests in actual wasm32-unknown-unknown modules on Node.js, in debug and optimized release builds. A pre-release allocation observer and deliberate residue controls cover vector slots, guarded collection growth and iteration, sensitive WKT strings and replacement, and map decode-error cleanup without changing application APIs.

- Clarify the ordinary Struct cleanup guarantee and the compatibility decision to retain its public HashMap field. Add pre-deallocation regression observations for current key and nested String buffers, including spare capacity, with ordinary destruction as a negative control. Vacated table storage and previously released ordinary allocations remain outside the guarantee; secret-bearing trees require guarded ownership from ingestion. No runtime API or cleanup behavior changes.

- Serialize guarded dynamic WKT views without constructing another owned payload tree. Preserve sorted object keys, last-duplicate semantics, representation JSON policies, diagnostics, and partial output on failure. Lazy lists validate all deferred wire children before writing and re-decode one child at a time to preserve error priority without retaining an aggregate decoded tree. Focused compatibility and allocation regressions cover eager, lazy, and owned view export.

### Fixed

- `buffa_descriptor::reflect::DynamicMessage::merge_with_options` now honours `DecodeOptions::clear_on_merge_error`. Previously the option was silently ignored, so a failed merge, including an input over the size limit, left old and partially merged fields behind; the message is now reset to empty and keeps its descriptor. The dropped values are not wiped. `DynamicMessage::merge` always uses the default options, so it never clears.

- `DecodeOptions::reject_unknown_fields()` now reaches `DynamicMessage` map entries: an unknown field inside a map entry, and an unknown closed-enum map value that a later known value replaces, are rejected as the generated decoders reject them. Under the default options these entries still decode, and an entry that ends with an unknown closed-enum value is still preserved as an unknown field.

- A crate that hand-wrote `impl ReflectElement` or `impl InspectElement` for an enum it generates must remove that impl, since codegen now emits it (a duplicate fails with E0119). Generated enums get `ReflectElement` whenever reflection is on, and `InspectElement` whenever a cleanup generation has reflection on, whatever the enum's openness and the reflection mode. A generation that reaches an enum through `extern_path` and resolves it as closed, while its owner opened it with an `open_enums_in` rule or uses bridge reflection, types repeated and map-value fields as the bare enum and previously failed to compile. Regenerate the crate that owns the enum with this release. If you cannot regenerate the owning crate, add an `open_enums_in` rule for the enum to the referencing generation, so that its fields use `EnumValue<E>`; this also keeps unknown enum values instead of treating them as unknown fields.

- `protoc-gen-reallyme-buffa-packaging` error responses now carry the advertised edition range as well as the supported features, so Buf reports the plugin's own configuration error instead of a protocol error about missing edition metadata.

- A custom bytes, string or repeated field type that is not a Rust type path, such as `_`, `()`, `[u8; 4]`, `&'static [u8]`, `impl Trait`, is now rejected at generation time with `CodeGenError::InvalidTypePath`, which the plugin returns in its response. Previously it was spliced into the generated struct and failed only when the consumer crate compiled. Associated-type projections such as `<A as Trait>::B` and parenthesized type paths remain supported, including equivalent spelling normalization for JSON policies. Name any other type through a type alias.

- The warning for a custom field-type rule that matched no field reads the same from the plugin and from `buffa-build`: it names the rule's kind and the option names instead of the internal `CodeGenConfig` list, and says that only the files of the current run are checked. For a rule that names an extension field it now says that custom field-type rules do not apply to extensions, which keep their default Rust type, through the new `names_extension` field of `CodeGenWarning::FieldTypeRuleMatchedNothing`.

- **A custom bytes or string field, sub-message or map entry whose declared length runs past the message that holds it is now rejected with `DecodeError::UnexpectedEof` before it is decoded.** Previously the length was checked only against the rest of the input, so a `decode_length_delimited` override (including the guarded `SecretString` and the guarded WKTs) could be handed the parent's following fields or later frames of a stream, a failed merge could keep those bytes in the field, and `max_message_size` did not bound the window. Generated code now decodes custom string and bytes fields through the new `decode_string_to_ctx` and `decode_bytes_to_ctx`; code generated by an earlier release keeps the old bound for those fields until it is regenerated, so regenerate all schemas with this release.

- **The `ProtoBytesJson` docs no longer suggest that `json_helpers::bytes::deserialize` keeps decoded bytes under the policy's control.** It decodes into a `Vec<u8>` that buffa allocates, drops a partly decoded buffer without wiping it on invalid input, and its error text can quote a symbol and offset from the input. A policy whose `deserialize_json` calls `bytes::deserialize` and that must wipe its plaintext should switch to a visitor that decodes with the new `bytes::decode_into` into storage it wipes, as the guarded fixture in `buffa-test` does. The docs now also list the copies no policy controls: the caller's serializer output, `serde_json`'s scratch buffer for escaped strings and `from_reader`, and `google.protobuf.Any` buffering through `serde_json::Value`.

- **A `json_policy_type` from another crate now works in `repeated` fields and `map` values, and separate generation runs no longer emit conflicting impls for it.** Generated code implemented `ProtoElemJson` for every listed policy type used as a `repeated` element or `map` value, which the orphan rule rejects (E0117) for a type defined in another crate, and which two runs into one crate (buf's per-directory strategy, or several `compile()` calls) each emitted (E0119); the only way out was `custom_type_impls=false` with a hand-written `ProtoElemJson` in the type's own crate. Those fields now go through new hidden buffa with-modules that wrap the element in a buffa-owned adapter and call the policy directly, so no JSON trait impl is generated or needed for a listed type. A guarded message's `sensitive::List`, `sensitive::Map` or custom collection still receives each decoded element directly, and a map key whose value fails is still erased. Vtable reflection, or reflection with message cleanup, still emits per-element impls, so there the type must stay crate-local or be supplied with `custom_type_impls=false`. A hand-written `ProtoElemJson` for a listed type is no longer called by generated code and can be removed; if it did not delegate to the type's policy, those fields' JSON now follows the policy. Errors for these fields no longer quote the input: a bad integer or `bool` map key reports `invalid protobuf map key`, and a bad base64 map key `invalid base64`.

- **The `ProtoStringJson` docs now state that a policy used for `map` keys must round-trip, and so be injective.** Runtime behaviour is unchanged: a key policy that maps two keys to one string has always produced a JSON object with a repeated key, whose import keeps only the last entry and silently loses the others, and a key that does not round-trip comes back altered or fails to import. Buffa does not check either property, so review any key policy that normalizes text (for example by changing case). The contract also states more precisely that a deserialized value must carry no state beyond the bytes or text it represents.

- **The JSON policy docs no longer claim that a policy governs every JSON use of a type, and codegen now warns that reflection bypasses policies.** A `ProtoBytesJson` / `ProtoStringJson` policy is consulted only by the generated serde impls. `DynamicMessage::try_from_message` works on any generated message, with or without generated reflection and including messages in the cleanup profile, and `DynamicMessage` JSON (like `to_dynamic` on a reflected message) prints a policy field as standard base64 or a plain string without calling the policy, so a policy that refuses export does not stop it and its import checks do not run; binary encoding and text format never call it either. A policy is therefore not a confidentiality boundary: keep reflective JSON, text and binary output of these messages away from sinks that must not see the values. The trait docs, `json_policy_type` docs, the guide and `docs/guarded-fields.md` now say so, and generating policy fields while JSON and reflection are both enabled produces `CodeGenWarning::JsonPolicyReflectionBypass`, naming the selected policy types in sorted, deduplicated order, printed by `buffa-build` as a `cargo:warning` and by the plugin on stderr. `buffa-build` and the plugin always print it, once per generation run that selects policy fields; unused rules do not warn. It is a reminder rather than a check: without reflection there is no warning, but the bypass remains.

- Preserve supported caller Serde defaults and input skips, including conditional attributes, when messages opt into cleanup. Reject unsupported input customizations with a fixed typed configuration error. Wipe transferred numeric JSON strings before release on both successful conversion and rejection without copying borrowed input.

- Reject ordinary repeated enum null elements in both JSON modes across Vec, custom and guarded collections. Preserve the explicit NullValue exception for older generated consumers using the updated canonical WKT provider through a defaulted Enumeration marker. Repeated NullValue output changes from the old enum-name spelling to canonical null in owned and borrowed JSON; nonzero unknown numbers remain numeric. Scalar, optional, map and non-human-readable serialization retain their existing behavior. External NullValue providers require regeneration or an explicit marker override, and newly generated canonical providers require the matching runtime.

- Keep custom string and bytes decode windows bounded after a delegated conversion fails, so recovery cannot expose following fields. Reject length prefixes that cross the enclosing message boundary even for empty payloads, while retaining valid in-frame empty values and supported noncanonical prefixes. JSON policy container tests use the runtime-selected map type with default features disabled, preserving custom-list coverage.

- Keep shared JSON rejection visitors hygienic in downstream crates and accept 128-bit visits when the value fits the existing 64-bit integer boundary. Wider rejected integers remain redacted, and rejected owned strings and bytes retain their opt-in wiping behavior.

- Enable std for the opt-in WKT and descriptor arbitrary features, as their generated derives require. Add isolated native and Wasm checks, including the guarded WKT combination, while retaining ordinary no_std coverage on bare-metal ARM. The feature-check script selects the repository's toolchain and configuration when called from another directory.

### Security

- Preserve full guarded-list allocation cleanup after upgrading zeroize: erase vacated element storage after destructors run, including inactive enum payloads. Verify both explicit clearing and destruction with allocator observations.

- Wipe the vacated element slots when zeroizing `UnknownFields` and `buffa-types` `ListValue`. Since zeroize 1.9.1, `Vec::zeroize` no longer overwrites an element's storage after its destructor runs, so unknown-field wire types, payload buffer pointers and capacities, `Value` kind tags and padding stayed in the retained buffer until it was reused or freed. Consumers of 0.1.2 whose lockfile resolved zeroize 1.9.1 are affected and should upgrade. `Struct` entries are still moved out of their hash table without wiping the vacated buckets, which remains a best-effort limitation.

- **Omit wrong-kind values from ProtoJSON decoding diagnostics.** A JSON value of the wrong kind (for example a string where a bool, number, object or array is expected, or a 128-bit integer from a non-JSON deserializer) is now rejected with an error that names the JSON kind but not the value at these positions: bool and string fields, including `optional`, repeated and map positions and oneof variants; the scalar, bytes and container helpers in `json_helpers`, including `proto_list` and the `enum_list` helpers; a generated message with a custom `Deserialize` (a oneof, an extension range or cleanup selection), including the descriptor types in `buffa-descriptor`; `BoolValue` and `StringValue`; `Timestamp`, `Duration` and `FieldMask`; `Struct`, `ListValue`, `Empty` and `Value`; and the same positions in `DynamicMessage`'s JSON codec. A rejected owned string or byte buffer is wiped first under buffa's `zeroize` feature.
  Generated `optional bool` and `optional string` fields now use the new `json_helpers::opt_bool` and `json_helpers::opt_string` modules, repeated and map fields of bool and default `String` values use `proto_seq` (or `proto_list` for a custom collection), `proto_map` and `proto_str_key_map`, and oneof bool and string variants deserialize through `proto_bool` and `proto_string`. An `optional` field of a custom `string_type` therefore no longer needs `serde` impls, and a oneof variant needs `Serialize` only; neither runs the type's own `Deserialize` any more. Code generated by this release requires the `buffa` runtime of the same release; regenerate existing bindings to pick up the changes.
  Generated messages with a custom `Deserialize`, `Struct`, `ListValue`, `Empty`, `Timestamp`, `Duration` and `FieldMask` ask a human-readable deserializer for any kind so that the visitor sees the token; other deserializers keep the typed request. The int32, uint32, int64, uint64, float, double and bytes helpers now read a 128-bit integer that fits in 64 bits as the 64-bit value it holds and reject a wider one with their fixed reason; `Value` and reflective numeric visitors likewise accept values inside their existing 64-bit boundary and redact wider rejections.
  Accepted JSON is unchanged, except that an object member named `$serde_json::private::Number` (the form in which `serde_json`'s `arbitrary_precision` feature delivers a number) is now rejected as a number by these message types and by `Empty`, even where unknown fields are otherwise ignored. The diagnostic for a `null` element or map value is unchanged.
  Still outside this change: a message with a derived `Deserialize` given a non-object, since `serde_json` formats the value before Buffa code runs; an element or map value of such a message type; a repeated message field given a non-array and a `map<string, Message>` field given a non-object; `Any` given a non-object; custom `string_type` elements in repeated and map fields, which keep their own `serde` impls; and the element visitors of `proto_enum`, `closed_enum` and the guarded enum helpers for 128-bit integers and bytes from non-JSON deserializers.

- **Omit the path from `FieldMask` JSON diagnostics.** The generated `FieldMask` quoted a rejected path in its JSON errors. A malformed path in JSON input is now reported as `invalid FieldMask path, expected comma-separated lowerCamelCase paths`, and a path with no JSON form (for example one decoded from binary input) is reported on output as `invalid FieldMask path, it has no proto3 JSON representation`, by the generated `FieldMask`, its view and `DynamicMessage`'s JSON codec alike. Accepted paths are unchanged.

- Integrate guarded-field and owned-view cleanup hardening with fixed JSON diagnostics. Preserve reviewed custom Serde, binary-format, and legal JSON-name compatibility; guarded messages with the reserved arbitrary-precision JSON name redact failures at their typed parser boundary.

### Known maintenance limitation

The optional `buffa-types/chrono` conversion API retains Chrono 0.4.45 for source compatibility. [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543.html) marks every Chrono version unmaintained and lists no patched release. Default runtime and WKT builds do not enable Chrono; the `chrono` feature and all-features builds do. Existing conversions remain available, and the warning is not suppressed. The existing `jiff` integration provides an alternative for consumers that can migrate.

[0.1.3]: https://github.com/reallyme/buffa/releases/tag/reallyme-v0.1.3

## [0.1.2] - 2026-11-14

### Changed

- Update the ProtoJSON base64 dependency to 0.23.1 with the same scalar engine and allocation-only features.

[0.1.2]: https://github.com/reallyme/buffa/releases/tag/reallyme-v0.1.2

## [0.1.1] - 2026-11-14

### Security

- Enum JSON value errors omit the input name or number. This covers generated enum values, enum field values and elements, registered enum extensions, and reflective enum values. The accepted values and lenient parsing behavior are unchanged. Regenerate existing bindings with the matching generator to update standalone enum deserialization.

- Generated eager, lazy, and owned views and view oneofs honor skip_debug by formatting only a redaction marker with the type name. Preserved unknown fields still round-trip. Regenerate existing bindings to replace their old Debug implementations.

- ProtoJSON decoding errors from map-key parsing, numeric conversion and range checks, base64 decoding, malformed Timestamp and Duration strings, and Any payload decoding omit rejected values. Accepted inputs are unchanged. Unknown-field diagnostics outside Any (including Serde-derived errors), parser and deserializer errors at other boundaries, and serialization errors are outside this change.
- With the runtime `zeroize` feature enabled, standalone enum deserialization wipes string allocations transferred by Serde, including on rejection. Deserializers retain responsibility for their own buffers.

### Added

- `borrowed_views_only_in` omits standard owned wrappers and their `HasMessageView` bindings for selected messages and their containers. Eager and lazy borrowed views remain available.
- `buffa::sensitive::GuardedOwnedView` holds eager views in private wiping wire storage. It encodes directly into that storage from owned messages and exposes only borrowed bytes and views.
- `DecodeOptions::clear_on_merge_error` clears the entire destination if a merge fails. Generated guarded messages wipe fields through their existing `Message::clear` implementation.

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
