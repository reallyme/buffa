# WASM runtime erasure tests

Run `scripts/test-wasm-erasure.sh` with the repository's pinned Rust toolchain,
the `wasm32-unknown-unknown` target installed, and Node.js 26. CI runs the same
command for debug and release builds. The release build uses the workspace's
LTO and single-codegen-unit configuration.

The unpublished `wasm-erasure-tests` fixture links the alloc-only runtime and
sensitive WKT family. Its explicit `runtime-tests` feature leaves the default
workspace dependency feature graph unchanged. No WebAssembly binding generator,
WASI emulation, or additional external test dependency is needed.

The actual Wasm module calls a JavaScript observer before its allocator releases
an explicitly registered allocation. The observer reads that live region from
the module's exported linear memory. It records counts, never buffer contents.
Each case gets a fresh instance and memory; memory views are recreated after
possible growth. Only fully initialized byte buffers and padding-free element
slots are registered. No freed memory or arbitrary struct padding is inspected.

Coverage includes:

- Generic vector cleanup after element destructors, spare capacity, and an empty
  vector retaining a nonempty allocation.
- Guarded-list growth, drop, and partially consumed double-ended iteration.
- Guarded-string live bytes and spare capacity.
- Nested guarded WKT keys and string values, duplicate-key replacement, and
  cleanup of a decoded map key when its value is invalid.
- Two negative controls: ordinary string destruction and vector cleanup without
  the final post-drop allocation wipe must leave observable residue.
- Invalid case rejection and recovery for a following valid request.
- A forced linear-memory growth before a dirty control, detecting an observer
  that accidentally retains a stale or detached memory view.

The same fixture cases also execute natively with
`cargo test -p wasm-erasure-tests --features runtime-tests`.
The existing Miri CI job runs these native cases, including the negative
controls, to check the allocation observer's unsafe boundaries.

These observations establish behavior for the tested allocations, builds, and
runtime. They do not establish universal physical erasure, cleanup of caller or
JavaScript copies, parser scratch storage, or arbitrary compiler temporaries.
Ordinary `Struct` hash-table buckets and historical ordinary-container growth
allocations remain outside the documented cleanup guarantee.
