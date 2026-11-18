# Ordinary Struct cleanup: guarantee and compatibility decision

Status: preserve the ordinary API for 0.1.3. Stronger lifecycle guarantees for
secret payloads require wipe-owning storage from their first ingestion. The
existing `buffa-types/sensitive` family provides that separate ownership model;
it does not promise erasure of all allocator or tree-node metadata. This design
does not introduce another container or change the ordinary field type.

## Current guarantee

With `buffa-types/zeroize`, an explicit call to ordinary
`google::protobuf::Struct::zeroize()`:

1. Recursively calls `Zeroize` on values while they still occupy their map
   entries, before draining can move inline numeric payloads out of those slots.
2. Drains entries, wipes each current key's String buffer, and cleans each value.
   Current String live bytes and spare capacity receive cleanup. Nested ordinary
   Structs have the same limited contract; ordinary ListValue uses the current
   vector-allocation wipe helper.
3. Cleans preserved unknown fields and leaves the public map empty and reusable.

This describes storage still reachable when cleanup begins and assumes cleanup
and destruction return normally. Ordinary WKTs do not implement automatic cleanup
on drop. A `Zeroizing<Struct>` schedules this same method when its wrapper drops;
it does not intercept earlier operations through the public map or child fields.

The guarantee excludes vacated hash-table storage, table metadata, padding,
allocations released during earlier growth, values removed or overwritten before
cleanup, and caller/parser/serializer copies. In particular, an empty map is not
evidence that all of its retained allocation is zero.

## Why a final cleanup patch cannot establish full erasure

`Struct.fields` is a public concrete `HashMap<String, Value>`, using the runtime's
hasher. With `std` it is a standard-library map; without `std` it is hashbrown.
Applications can use `insert`, `remove`, `entry`, `reserve`, `clear`, and direct
mutation of each ordinary Value without passing through a Buffa ownership hook.
Those operations may release or overwrite data before `Struct::zeroize()` runs.
No later call can safely revisit an allocation that has already been released.

Standard `HashMap` exposes entry references and a logical capacity, not a mutable
byte slice covering its allocation. Its `drain` method deliberately retains the
table for reuse. Neither draining nor shrinking promises erasure. A size or an
entry pointer is insufficient to establish an allocation's base, byte layout, or
ownership. The supported public hashbrown 0.17.1 map API does not provide a
portable replacement for those missing standard-library guarantees either.

Wiping keys in place through a shared reference would violate Rust's aliasing
rules. Mutating a key's hash/equality while it remains in the map also violates
the map's documented key invariant. Inferring private bucket layouts, casting a
standard map to hashbrown, scanning freed memory, or assuming all typed padding
is initialized are not acceptable implementation strategies.

## Compatibility options

| Option | Compatibility | Result |
| --- | --- | --- |
| Keep explicit ordinary cleanup and its bounded guarantee | Preserves current source and behavior | Chosen for ordinary WKTs in 0.1.3. |
| Add convenience mutation methods or wrap the ordinary Struct in Zeroizing | Can be additive | Public map and child-field access still bypass lifecycle cleanup; cannot justify a stronger guarantee. |
| Drain, shrink, replace, or drop the existing map | Can preserve the field type | Releasing a table does not wipe it, and earlier released buffers remain unreachable. |
| Replace the ordinary field with a private guarded map and guarded child values | Changes concrete field types and mutation/iteration APIs | Could enforce ownership, but duplicates the sensitive WKT family and requires an explicit breaking migration. |
| Use a specialized map allocator | Changes allocator/type or dependency requirements | Can address allocation release only; entry removal, replacement, and independently allocated String payloads still need cleanup ownership. Standard allocator constructors are nightly-only in Rust 1.100. |
| Require an application-wide wiping allocator | Changes application policy | A separate application decision, not a guarantee an ordinary library type can impose; retained allocations still need explicit cleanup. |

A guarded replacement must also account for map iteration/serialization order,
lookup and entry APIs, capacity/performance behavior, `std`/`no_std`, reflection,
JSON, and code generation. Wire/JSON semantic compatibility does not establish
identical serialized bytes for an application that authenticates those bytes.

## Decision and migration boundary

Keep ordinary WKTs source-compatible and describe their explicit cleanup
accurately. Do not add a new feature flag that silently changes their concrete
field types or makes a larger promise only under `no_std`.

For secret-bearing dynamic trees, select `buffa-types/sensitive` and configure
the guarded WKT extern paths before decoding or constructing the tree, as shown
in [the guarded-fields guide](guarded-fields.md#guard-well-known-types).
Converting an already populated ordinary tree later cannot erase its allocation
history. Guarded storage also does not erase arbitrary application copies or
compiler temporaries.

No new production implementation is justified by this investigation. A future
replacement for the ordinary API would require an explicitly reviewed breaking
design and migration plan. The current upstream proposal keeps the whole-message
cleanup profile as a design discussion, so no existing upstream PR contains this
ordinary WKT cleanup extension to amend.

## Validation boundary

`buffa-types/tests/zeroized_vec_slots.rs` observes current key and nested String
payload allocations before deallocation, including initialized spare capacity.
Ordinary destruction is a negative control and must leave detectable residue;
explicit cleanup must erase it. It also checks that the map remains reusable.
The observer never inspects table buckets, typed padding, or released memory.

Any future stronger design needs separate observations for growth, removal,
replacement, error cleanup, retained empty storage, and final release, with
negative controls that remain sound when cleanup fails. Native/Miri and actual
optimized Wasm execution must distinguish the tested storage from unowned copies;
a cross-compilation alone is not runtime erasure evidence.

## API references

- [Standard HashMap, including key invariants](https://doc.rust-lang.org/std/collections/struct.HashMap.html).
- [HashMap drain retains its allocation](https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.drain).
- [Standard allocator constructors](https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.with_hasher_in).
- [hashbrown 0.17.1 HashMap API](https://docs.rs/hashbrown/0.17.1/hashbrown/struct.HashMap.html).
