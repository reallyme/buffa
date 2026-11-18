# Borrowed inspection

Enable `buffa-descriptor/reflect` and `buffa-descriptor/zeroize` for the
`reflect::InspectMessage`, `InspectValueRef`, `InspectList` and `InspectMap`
interfaces. These inspect live owners and borrow strings, bytes, children and
collections. They offer neither serialization nor dynamic snapshots. The
ordinary reflection API and its feature defaults remain unchanged.

Scalar inspection slots wipe their stored copies on drop. Explicitly extracting
or copying a scalar transfers responsibility for that copy to the caller.
Borrowed text and wire storage remain under their source owner's cleanup.
Debug output is redacted. A message implementation must reject descriptors from
another message or pool, even if structurally identical. Unset explicit-presence
fields report declared protobuf defaults. Hand-written implementations can use
this runtime API directly. Generated implementations require the separate
companion cleanup-codegen contribution; this runtime topic alone does not add
`generate_zeroize` or `zeroize_in` to the generator.
