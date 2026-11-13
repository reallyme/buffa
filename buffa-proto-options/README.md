# reallyme-buffa-proto-options

<!-- Modified by ReallyMe Technologies Limited. -->

This is the **0.1.0** fork maintained by **ReallyMe Technologies Limited**, distributed under Apache-2.0. The upstream license and attribution notices are retained. See [fork provenance and migration](https://github.com/reallyme/buffa/blob/reallyme-v0.1.0/FORK.md) for the package aliases, matching generator release, cleanup features and limits.

Rust types for the custom protobuf options of [buffa](https://crates.io/crates/reallyme-buffa).

`buffa/ext/options.proto`, in this crate's `protos/` directory, declares one option for each kind of schema element that buffa generates Rust for: `(buffa.ext.file)`, `(buffa.ext.message)`, `(buffa.ext.field)`, `(buffa.ext.oneof)`, `(buffa.ext.enum)`, and `(buffa.ext.enum_value)`. Each option is a message, and a setting is a field of that message, so an empty option message leaves the generated code unchanged.

This crate provides the option messages and the extension constants (`buffa_proto_options::FIELD` and the rest) that read an option from a descriptor through `buffa::ExtensionSet`. The crate documentation has an example.

The only required dependency is `buffa`.
