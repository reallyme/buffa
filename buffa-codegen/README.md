# reallyme-buffa-codegen

This is the **0.1.0** fork maintained by **ReallyMe Technologies Limited**, distributed under Apache-2.0. The upstream license and attribution notices are retained. See [fork provenance and migration](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/FORK.md) for the package aliases, matching generator release, cleanup features and limits.

Generates Rust message, enum, view and codec implementations from protobuf descriptors. Most consumers should use `reallyme-buffa-build` in `build.rs` or the `reallyme-protoc-gen-buffa` CLI, which configure this engine for their respective workflows.

For direct integration:

```toml
[dependencies]
buffa-codegen = { package = "reallyme-buffa-codegen", version = "=0.1.0" }
```

See the [configuration guide](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/docs/guide.md).
