# reallyme-buffa-build

<!-- Modified by ReallyMe Technologies Limited. -->

This is the **0.1.0** fork maintained by **ReallyMe Technologies Limited**, distributed under Apache-2.0. The upstream license and attribution notices are retained. See [fork provenance and migration](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/FORK.md) for the package aliases, matching generator release, cleanup features and limits.

Build-time integration for compiling Protocol Buffers schemas into buffa Rust
types. It runs from your crate's `build.rs`; generated messages use the
separate `buffa` runtime crate.

## Protobuf compiler

By default, `buffa-build` runs `protoc`. Install it on `PATH` or set the
`PROTOC` environment variable. The minimum version is v21.12; schemas that use
editions 2023 need v27.0, and schemas that use editions 2024 need v32.0.

To use `buf` instead, install it and select it with `.use_buf()`. That mode
needs a `buf.yaml` at the crate root, ignores `.includes()`, and takes
`.files()` paths relative to the buf module directory. You can also pass a
precompiled descriptor set with `.descriptor_set()`.

See the [prerequisites section of the user guide](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/docs/guide.md#prerequisites)
for how to install either compiler.

## Quick start

Add the runtime and build dependencies. Add `buffa-types` too when your
schemas use protobuf well-known types such as `Timestamp` or `Any`.

```sh
cargo add reallyme-buffa@=0.1.0 --rename buffa
cargo add --build reallyme-buffa-build@=0.1.0 --rename buffa-build
cargo add reallyme-buffa-types@=0.1.0 --rename buffa-types           # optional: the well-known types
```

Compile your schemas and generate a module tree:

```rust,ignore
// build.rs
fn main() {
    buffa_build::Config::new()
        .files(&["proto/my_service.proto"])
        .includes(&["proto/"])
        .include_file("_include.rs")
        .compile()
        .unwrap();
}
```

Include the generated modules in your crate:

```rust,ignore
// src/lib.rs
mod proto {
    include!(concat!(env!("OUT_DIR"), "/_include.rs"));
}
```

`include_file` creates the nested module tree for the protobuf packages,
including packages that refer to one another.

See the [user guide](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/docs/guide.md#using-buffa-build-in-buildrs)
for the full set of code generation options.
