# reallyme-buffa-remote-derive

This is the **0.1.0** fork maintained by **ReallyMe Technologies Limited**, distributed under Apache-2.0. The upstream license and attribution notices are retained. See [fork provenance and migration](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/FORK.md) for the package aliases, matching generator release, cleanup features and limits.

Derive macros for crate-local wrappers around external string, byte and collection representations. The Rust macro path remains `buffa_remote_derive`; these derives implement the storage traits and do not make an external representation wipe its allocations.

```toml
[dependencies]
buffa = { package = "reallyme-buffa", version = "=0.1.0" }
buffa-remote-derive = { package = "reallyme-buffa-remote-derive", version = "=0.1.0" }
```

See the [API reference](https://docs.rs/reallyme-buffa-remote-derive) and [guarded representation requirements](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/docs/guarded-fields.md).
