# Owned JSON input cleanup

With `buffa/json` and `buffa/zeroize`, numeric and enum visitors wipe text
transferred through Serde's owned-string callback before releasing it, whether
decoding succeeds or fails. Rejection visitors also wipe transferred strings
and byte buffers. Without `zeroize`, allocation and decoding behavior remain
unchanged.

This is an ownership boundary, not a promise that every parser copy is erased.
Borrowed input, deserializer scratch, earlier parser allocations and application
output remain with their owners. A parser that fails before invoking a visitor
also owns its diagnostic and storage. Applications must choose suitable parsers
and clear their own buffers. Accepted string/bytes payloads retain their existing
storage behavior; this feature does not convert them into guarded containers.
