# Guarded runtime storage

Enable `buffa/zeroize` to use `buffa::sensitive::List`, `Map` and `StackText`.
The ordinary protobuf containers and their defaults remain unchanged.

`List<T>` cleans values before destruction and wipes the current allocation,
including vacated slots and spare capacity. Growth allocates replacement storage,
moves values, then wipes the old buffer before releasing it. Mutable access is
limited to elements to avoid slice methods creating unowned temporary copies.

`Map<K, V>` uses deterministic ordered lookup and stable wipe-owned allocations
for keys and values. Inserting a duplicate key cleans the redundant key and
replaced value. Balancing the tree moves pointers, not payload owners. This costs
more allocations than the ordinary hash map and is an explicit opt-in trade-off.
Key and value cleanup follows each type's `Zeroize` contract before destruction;
the map does not promise a final raw wipe of boxed headers, padding, or bytes
that a custom destructor writes afterwards.

Borrowed view containers erase their owned reference slots, not the caller's
wire buffer. Values transferred out by iterators become the caller's cleanup
responsibility. Bounded text formatting writes directly into a wiping owner.

These guarantees assume element cleanup and destructors return normally.
Allocator exhaustion follows the existing infallible collection contracts.
Parser scratch, caller buffers, application clones and earlier allocations from
ordinary `Vec`/`String` growth are outside these owners' guarantees. Enabling the
feature also adds explicit `Zeroize` support to unknown fields and runtime
message/enum slots; ordinary values do not automatically acquire `Drop` cleanup.
Explicit lazy-view cleanup also wipes its current fragment-reference allocation
before reset. Earlier ordinary lazy-vector growth allocations are excluded.
