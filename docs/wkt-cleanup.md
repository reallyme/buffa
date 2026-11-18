# Explicit cleanup for ordinary well-known types

`buffa-types/zeroize` adds `Zeroize` to `Timestamp`, `Duration`, `Value`,
`ListValue`, `Struct`, the Value kind and canonical `NullValue`. Ordinary public
fields and type names remain unchanged. Cleanup is explicit: these types do
not acquire automatic cleanup on drop, and their Debug/Serde behavior is unchanged.

Live values and owned string payloads are cleaned recursively. Current vector
allocations are wiped after elements are dropped, including spare capacity.
Unknown fields participate in recursive cleanup.

`Struct.fields` remains the existing hash map for source compatibility. Cleanup
wipes live values in place before draining and cleans removed keys and values.
It cannot promise that vacated buckets or allocations already freed by earlier
growth are erased. The same earlier-growth exclusion applies to ordinary
vectors and strings. Caller buffers, parser scratch and application copies are
outside the guarantee. For stronger ownership through growth, select a separate
wipe-owning representation rather than relying on a final cleanup call.
