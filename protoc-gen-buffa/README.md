# reallyme-protoc-gen-buffa

This is the **0.1.0** fork maintained by **ReallyMe Technologies Limited**, distributed under Apache-2.0. The upstream license and attribution notices are retained. See [fork provenance and migration](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/FORK.md) for the package aliases, matching generator release, cleanup features and limits.

The protoc plugin that generates Rust protobuf messages and codecs. Install the registry package below; the executable has a distinct name so it can coexist with the upstream plugin.

```sh
cargo install reallyme-protoc-gen-buffa --version '=0.1.0' --locked
protoc-gen-reallyme-buffa --version
```

The expected version output is `protoc-gen-reallyme-buffa 0.1.0`. Use `local: protoc-gen-reallyme-buffa` in Buf, or `--reallyme-buffa_out` with protoc. Regenerate schemas with this generator when switching the runtime to `reallyme-buffa`.

For a module tree spanning protobuf packages, use the matching `reallyme-protoc-gen-buffa-packaging` package. See the [Buf configuration and migration instructions](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/FORK.md).
