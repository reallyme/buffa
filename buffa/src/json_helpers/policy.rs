//! JSON policies for custom `bytes` and `string` representations.
//!
//! Explicit public helpers invoke these policies directly. Companion codegen
//! selects them for generated fields; this runtime API also supports hand-written
//! serde implementations.
//!
//! A representation listed with `buffa_build::Config::json_policy_type` (plugin
//! option `json_policy_type=<rust_type>`) implements [`ProtoBytesJson`] or
//! [`ProtoStringJson`]. The `serde` impls that codegen generates then route
//! the type's JSON through the trait: singular, optional, repeated and oneof
//! fields, `map` keys and values, and the matching fields of generated views,
//! lazy views and owned views. The type's own `serde` impls, if any, are not
//! consulted.
//!
//! Descriptor-driven reflection is outside the policy. JSON produced or read
//! through `buffa-descriptor` (for example `DynamicMessage`'s serde impls, or
//! a generated message converted to a `DynamicMessage` with
//! `ReflectMessage::to_dynamic` or
//! `DynamicMessage::try_from_message`, which works whether or not reflection
//! is generated) encodes each field by its protobuf type, as standard base64
//! or a plain string, and never calls the policy in either direction: a
//! policy that refuses export does not stop it, and a policy's import checks
//! do not run on it. Binary encoding and text format never call the policy
//! either, so a policy is not a confidentiality boundary.
//!
//! Every position is served by a buffa with-module, adapter or seed that is
//! generic over the policy trait, so generated code implements no JSON trait
//! for the policy type. A `repeated` field uses `repeated_bytes_policy` or
//! `repeated_string_policy` for a `Vec`, and `list_bytes_policy` or
//! `list_string_policy` for a custom collection (including the guarded
//! `sensitive::List`), which receives each element directly. A `map` field
//! whose value is a policy type uses `map_bytes_policy` or
//! `map_string_policy`, with a suffix naming the key shape: none for integer,
//! `bool` and `String` keys, `_serde_key` for a custom string key without a
//! policy, `_policy_key` for a policy key, and `_bytes_key` for `string` keys
//! normalized to bytes. A policy key with any other value uses
//! `policy_str_key_map`.
//!
//! The legacy `repeated_*_policy` helpers retain ordinary Vec staging for
//! compatibility: earlier growth allocations and scalar slots are not covered
//! by a final cleanup call. Ordinary map storage has the same relocation
//! limitation. The direct-owner `list_*` helpers and wipe-owning MapStorage
//! implementations avoid these stages when the caller selects them.
//!
//! The functions are type-level: serialization receives the field's borrowed
//! `&[u8]` or `&str`, which is all a view holds, so one policy governs the
//! owned message and its views alike. A policy cannot depend on per-value
//! state that a view does not carry.

use super::{ProtoElemJson, ProtoElemSeed};
use crate::map_codec::MapStorage;
use crate::ProtoList;
use alloc::vec::Vec;
use core::fmt::Display;
use core::marker::PhantomData;
use core::str::FromStr;
use serde::de::{DeserializeOwned, DeserializeSeed, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

/// Fixed rejection reason for views whose message export policy forbids JSON.
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ViewJsonErrorReason {
    /// Export must use a policy-approved owned message instead.
    #[error("protobuf JSON serialization of this view is disabled")]
    Denied,
}

/// Protobuf JSON encoding for a custom [`ProtoBytes`](crate::ProtoBytes)
/// representation, selected with `json_policy_type`.
///
/// There is no default implementation and no blanket impl: a listed type that
/// does not implement this trait fails to compile, naming the trait.
/// The impl may live in the crate that defines the type: generated code needs
/// no further JSON trait impl for it in any position.
///
/// The policy governs explicit helper calls and the `serde` impls that codegen generates.
/// Descriptor-driven reflection, such as `DynamicMessage` JSON or a generated
/// message converted to a `DynamicMessage`, encodes and decodes the field by
/// its protobuf type and never calls the policy, in either direction. A
/// policy may reject export by returning an error from `serialize_json`, but
/// that blocks helper-based and generated export: reflective JSON still prints the
/// field as standard base64, so refusing export is not a confidentiality
/// boundary.
///
/// [`json_helpers::bytes`](super::bytes) provides buffa's standard base64
/// encoding for an implementation that keeps the ProtoJSON format:
///
/// - [`bytes::serialize`](super::bytes::serialize) streams the encoding into
///   the serializer through `collect_str`, without a heap `String` of its own.
/// - [`bytes::decode_into`](super::bytes::decode_into) decodes a borrowed
///   string once into a buffer the implementation provides, so the decoded
///   bytes land only in storage it owns.
/// - [`bytes::deserialize`](super::bytes::deserialize) is the convenience
///   path: it decodes into a `Vec<u8>` that buffa allocates and passes it to
///   `From<Vec<u8>>`, so the implementation does not choose where the decoded
///   bytes first land, and a partly decoded buffer is dropped without wiping
///   on invalid input. Both helpers report fixed error reasons without
///   quoting invalid input bytes or offsets.
///
/// # Copies outside the policy
///
/// A policy governs only what it allocates itself. These copies are made
/// around it, and no policy can prevent or wipe them:
///
/// - Serialized text goes to the serializer's output, which belongs to the
///   caller. A growing writer such as the `Vec` behind `serde_json::to_vec` or
///   `to_string` reallocates and frees earlier copies without wiping them. A
///   serializer that does not stream `collect_str`, such as
///   `serde_json::to_value`, builds its own `String`. While streaming,
///   `bytes::serialize` stages up to 1 KiB of encoded text in a stack buffer
///   that is not wiped.
/// - `serde_json` lends the visitor a `&str` borrowed from the input only when
///   it reads from a slice or `str` and the string contains no escape
///   sequence. An escaped string (for example one containing `\/`) and every
///   string read through `serde_json::from_reader` are first copied into
///   `serde_json`'s internal scratch buffer, which is not wiped.
/// - A message inside a `google.protobuf.Any` passes through a
///   `serde_json::Value` in both directions, which holds its own copy of each
///   field's JSON text. On import the message is then re-encoded into the
///   `Any`'s `value` bytes, a plain `Vec<u8>`.
/// - Descriptor-driven reflection (`to_dynamic`,
///   `DynamicMessage::try_from_message`) encodes the message and copies each
///   field into a `DynamicMessage`'s ordinary `Vec<u8>` or `String` storage,
///   which no policy or representation wipes, besides bypassing the policy
///   as described above.
///
/// # Contract
///
/// - `serialize_json` receives the field's bytes, from the owned value or from
///   a view, and must produce protobuf JSON for `bytes` (a base64 string) or
///   fail.
/// - `deserialize_json` never receives JSON `null`. Generated code handles
///   `null` before calling it: an implicit-presence field becomes the default
///   value, an optional field or oneof variant becomes unset, and a `null`
///   element of a `repeated` field or `map` value is rejected.
/// - A deserialized value must equal one built with `From<Vec<u8>>` from the
///   bytes it represents (its `as_ref()`), so it carries no state beyond its
///   bytes.
pub trait ProtoBytesJson: crate::ProtoBytes {
    /// Serialize a field's bytes as protobuf JSON.
    ///
    /// # Errors
    ///
    /// Returns the serializer's error, or a policy error built with
    /// [`serde::ser::Error::custom`].
    fn serialize_json<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>;

    /// Deserialize a non-null protobuf JSON `bytes` value.
    ///
    /// # Errors
    ///
    /// Returns the deserializer's error for malformed input, or a policy error
    /// built with [`serde::de::Error::custom`].
    fn deserialize_json<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}

/// Protobuf JSON encoding for a custom [`ProtoString`](crate::ProtoString)
/// representation, selected with `json_policy_type`.
///
/// There is no default implementation and no blanket impl: a listed type that
/// does not implement this trait fails to compile, naming the trait.
/// The impl may live in the crate that defines the type: generated code needs
/// no further JSON trait impl for it in any position.
///
/// The policy governs explicit helper calls and the `serde` impls that codegen generates.
/// Descriptor-driven reflection, such as `DynamicMessage` JSON or a generated
/// message converted to a `DynamicMessage`, encodes and decodes the field by
/// its protobuf type and never calls the policy, in either direction. A
/// policy may reject export by returning an error from `serialize_json`, but
/// that blocks helper-based and generated export: reflective JSON still prints the
/// field as a plain string, so refusing export is not a confidentiality
/// boundary.
///
/// # Contract
///
/// - `serialize_json` receives the field's text, from the owned value or from
///   a view, and must produce a JSON string or fail.
/// - `deserialize_json` never receives JSON `null`. Generated code handles
///   `null` before calling it: an implicit-presence field becomes the default
///   value, an optional field or oneof variant becomes unset, and a `null`
///   element of a `repeated` field or `map` value is rejected.
/// - A deserialized value must equal one built with `From<String>` from the
///   text it represents (its `as_ref()`), so it carries no state beyond its
///   text.
/// - A policy used for a `map` key must serialize as a JSON string. Maps with
///   enum values do not support policy keys because their unknown-enum
///   filtering needs a separate decoding path; codegen rejects that shape.
/// - A policy used for a `map` key must round-trip: for every key it exports,
///   `deserialize_json` of the exported string returns an equal key. That
///   makes it injective, as it must be: if two keys export the same string,
///   the JSON object repeats a key, buffa's import keeps only the last entry
///   and silently loses the others, and other parsers may reject the object.
///   A key that does not round-trip comes back altered or fails to import.
///   Buffa does not check either requirement; the serializer may reject a key
///   that is not a string.
pub trait ProtoStringJson: crate::ProtoString {
    /// Serialize a field's text as protobuf JSON.
    ///
    /// # Errors
    ///
    /// Returns the serializer's error, or a policy error built with
    /// [`serde::ser::Error::custom`].
    fn serialize_json<S: Serializer>(text: &str, serializer: S) -> Result<S::Ok, S::Error>;

    /// Deserialize a non-null protobuf JSON `string` value.
    ///
    /// # Errors
    ///
    /// Returns the deserializer's error for malformed input, or a policy error
    /// built with [`serde::de::Error::custom`].
    fn deserialize_json<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}

/// Generates the generated-code plumbing for one policy trait.
macro_rules! policy_plumbing {
    (
        trait: $trait:ident,
        borrowed: $borrowed:ty,
        owned: $owned:ident,
        singular: $singular:ident,
        optional: $optional:ident,
        adapter: $adapter:ident,
        seq_adapter: $seq_adapter:ident,
        seed: $seed:ident,
        $(,)?
    ) => {
        /// Owned value deserialized through its policy, so that
        /// `Option<_>` handles JSON `null` before the policy runs.
        struct $owned<T>(T);

        impl<'de, T: $trait> Deserialize<'de> for $owned<T> {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                T::deserialize_json(d).map($owned)
            }
        }

        /// Serde with-module for an implicit-presence field whose type implements
        /// this policy trait. JSON `null` deserializes to the default value.
        #[doc(hidden)]
        pub mod $singular {
            use super::$trait;
            use serde::{Deserializer, Serializer};

            /// Serialize through the type's policy.
            pub fn serialize<T: $trait, S: Serializer>(value: &T, s: S) -> Result<S::Ok, S::Error> {
                T::serialize_json(value.as_ref(), s)
            }

            /// Deserialize through the type's policy, mapping `null` to the
            /// default value.
            pub fn deserialize<'de, T: $trait, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
                <Option<super::$owned<T>> as serde::Deserialize>::deserialize(d)
                    .map(|value| value.map(|owned| owned.0).unwrap_or_default())
            }
        }

        /// Serde with-module for an explicit-presence field whose type implements
        /// this policy trait. JSON `null` deserializes to `None`.
        #[doc(hidden)]
        pub mod $optional {
            use super::$trait;
            use serde::{Deserializer, Serializer};

            /// Serialize through the type's policy.
            pub fn serialize<T: $trait, S: Serializer>(
                value: &Option<T>,
                s: S,
            ) -> Result<S::Ok, S::Error> {
                match value {
                    Some(v) => T::serialize_json(v.as_ref(), s),
                    None => s.serialize_none(),
                }
            }

            /// Deserialize through the type's policy, mapping `null` to `None`.
            pub fn deserialize<'de, T: $trait, D: Deserializer<'de>>(
                d: D,
            ) -> Result<Option<T>, D::Error> {
                <Option<super::$owned<T>> as serde::Deserialize>::deserialize(d)
                    .map(|value| value.map(|owned| owned.0))
            }
        }

        /// Serializes a borrowed value through the policy of `T`, for generated
        /// views and oneof variants.
        #[doc(hidden)]
        pub struct $adapter<'a, T: ?Sized>(&'a $borrowed, PhantomData<fn() -> T>);

        impl<'a, T: $trait> $adapter<'a, T> {
            /// Wrap a borrowed value for serialization under `T`'s policy.
            pub fn new(value: &'a $borrowed) -> Self {
                Self(value, PhantomData)
            }
        }

        impl<T: $trait> Serialize for $adapter<'_, T> {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                T::serialize_json(self.0, s)
            }
        }

        /// Serializes borrowed repeated elements through the policy of `T`, for
        /// generated views.
        #[doc(hidden)]
        pub struct $seq_adapter<'a, T: ?Sized>(&'a [&'a $borrowed], PhantomData<fn() -> T>);

        impl<'a, T: $trait> $seq_adapter<'a, T> {
            /// Wrap borrowed elements for serialization under `T`'s policy.
            pub fn new(elements: &'a [&'a $borrowed]) -> Self {
                Self(elements, PhantomData)
            }
        }

        impl<T: $trait> Serialize for $seq_adapter<'_, T> {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeSeq as _;
                let mut seq = s.serialize_seq(Some(self.0.len()))?;
                for element in self.0 {
                    seq.serialize_element(&$adapter::<T>::new(element))?;
                }
                seq.end()
            }
        }

        /// Deserializes a non-null value through the policy of `T`, for generated
        /// oneof variants.
        #[doc(hidden)]
        pub struct $seed<T>(PhantomData<fn() -> T>);

        impl<T> $seed<T> {
            /// Create the seed.
            pub fn new() -> Self {
                Self(PhantomData)
            }
        }

        impl<T> Default for $seed<T> {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<'de, T: $trait> DeserializeSeed<'de> for $seed<T> {
            type Value = T;
            fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
                T::deserialize_json(d)
            }
        }
    };
}

policy_plumbing! {
    trait: ProtoBytesJson,
    borrowed: [u8],
    owned: OwnedBytes,
    singular: bytes_policy,
    optional: opt_bytes_policy,
    adapter: BytesPolicyJson,
    seq_adapter: BytesPolicySeqJson,
    seed: BytesPolicySeed,
}

policy_plumbing! {
    trait: ProtoStringJson,
    borrowed: str,
    owned: OwnedString,
    singular: string_policy,
    optional: opt_string_policy,
    adapter: StringPolicyJson,
    seq_adapter: StringPolicySeqJson,
    seed: StringPolicySeed,
}

// ── Containers ───────────────────────────────────────────────────────────────
//
// `repeated` elements and `map` values go through buffa-owned wrappers, so
// generated code implements no trait for the policy type, which may live in
// another crate. The codecs below are private: each public module fixes one
// value codec and one key codec and states its bounds with public traits.
//
// The `list_*` modules and the map modules decode straight into the field's
// collection, so a wipe-owning collection (a guarded message's
// `sensitive::List` or `sensitive::Map`) governs every allocation that holds
// a decoded element, including growth and cleanup after a later error. A map
// key decoded before its value fails is erased through the collection's
// `storage_erase_key` hook.

/// A type-only marker that leaves auto traits unconstrained.
type Marker<T> = PhantomData<fn() -> T>;

/// How a container element or map value reaches JSON.
trait ValueCodec<V> {
    fn serialize<S: Serializer>(value: &V, s: S) -> Result<S::Ok, S::Error>;
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<V, D::Error>;
}

/// Values through [`ProtoBytesJson`].
struct BytesValues;

impl<V: ProtoBytesJson> ValueCodec<V> for BytesValues {
    fn serialize<S: Serializer>(value: &V, s: S) -> Result<S::Ok, S::Error> {
        V::serialize_json(value.as_ref(), s)
    }
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<V, D::Error> {
        V::deserialize_json(d)
    }
}

/// Values through [`ProtoStringJson`].
struct StringValues;

impl<V: ProtoStringJson> ValueCodec<V> for StringValues {
    fn serialize<S: Serializer>(value: &V, s: S) -> Result<S::Ok, S::Error> {
        V::serialize_json(value.as_ref(), s)
    }
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<V, D::Error> {
        V::deserialize_json(d)
    }
}

/// An element wrapped so that `ProtoElemSeed` rejects `null` before the
/// codec runs, exactly as for every other container element. The wrapper
/// only moves the element; it never copies its storage.
struct Elem<VC, V>(V, Marker<VC>);

impl<VC: ValueCodec<V>, V> ProtoElemJson for Elem<VC, V> {
    fn serialize_proto_json<S: Serializer>(v: &Self, s: S) -> Result<S::Ok, S::Error> {
        VC::serialize(&v.0, s)
    }
    fn deserialize_proto_json<'de, D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        VC::deserialize(d).map(|v| Elem(v, PhantomData))
    }
}

/// Serializes a borrowed element through its codec.
struct ValueJson<'a, VC, V>(&'a V, Marker<VC>);

impl<VC: ValueCodec<V>, V> Serialize for ValueJson<'_, VC, V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        VC::serialize(self.0, s)
    }
}

fn serialize_seq<VC: ValueCodec<T>, T, S: Serializer>(v: &[T], s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq as _;
    let mut seq = s.serialize_seq(Some(v.len()))?;
    for element in v {
        seq.serialize_element(&ValueJson::<VC, T>(element, PhantomData))?;
    }
    seq.end()
}

/// Decodes a sequence into a `Vec`, pre-sized from a clamped hint, as
/// [`proto_seq`](super::proto_seq) does.
fn deserialize_vec<'de, VC, T, D>(d: D) -> Result<Vec<T>, D::Error>
where
    VC: ValueCodec<T>,
    D: Deserializer<'de>,
{
    struct Vis<VC, T>(Marker<(VC, T)>);
    impl<'de, VC: ValueCodec<T>, T> serde::de::Visitor<'de> for Vis<VC, T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("an array or null")
        }
        fn visit_unit<E>(self) -> Result<Vec<T>, E> {
            Ok(Vec::new())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::with_capacity(super::clamp_size_hint(seq.size_hint()));
            while let Some(element) =
                seq.next_element_seed(ProtoElemSeed::<Elem<VC, T>>(PhantomData))?
            {
                out.push(element.0);
            }
            Ok(out)
        }
        crate::reject_other_json_kinds!(
            bool, i64, u64, i128, u128, f64, str, string, bytes, byte_buf, map
        );
    }
    d.deserialize_any(Vis::<VC, T>(PhantomData))
}

/// Decodes a sequence straight into a custom collection, as
/// [`proto_list`](super::proto_list) does: no intermediate `Vec`, and no
/// capacity hint forces the collection to allocate before its own policy
/// chooses to.
fn deserialize_list<'de, VC, T, C, D>(d: D) -> Result<C, D::Error>
where
    VC: ValueCodec<T>,
    C: ProtoList<T>,
    D: Deserializer<'de>,
{
    struct Vis<VC, T, C>(Marker<(VC, T, C)>);
    impl<'de, VC: ValueCodec<T>, T, C: ProtoList<T>> serde::de::Visitor<'de> for Vis<VC, T, C> {
        type Value = C;
        fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("an array or null")
        }
        fn visit_unit<E>(self) -> Result<C, E> {
            Ok(C::default())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<C, A::Error> {
            let mut out = C::default();
            while let Some(element) =
                seq.next_element_seed(ProtoElemSeed::<Elem<VC, T>>(PhantomData))?
            {
                ProtoList::push(&mut out, element.0);
            }
            Ok(out)
        }
        crate::reject_other_json_kinds!(
            bool, i64, u64, i128, u128, f64, str, string, bytes, byte_buf, map
        );
    }
    d.deserialize_any(Vis::<VC, T, C>(PhantomData))
}

/// How a map key reaches JSON.
trait KeyCodec<K> {
    fn serialize<S: Serializer>(key: &K, s: S) -> Result<S::Ok, S::Error>;
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<K, D::Error>;
}

/// Integer, `bool` and `String` keys: written with `Display`, read with
/// `FromStr` from the borrowed key text, as in
/// [`proto_map`](super::proto_map), with a fixed error that never echoes the
/// key.
struct DisplayKeys;

impl<K: Display + FromStr> KeyCodec<K> for DisplayKeys {
    fn serialize<S: Serializer>(key: &K, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(key)
    }
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<K, D::Error> {
        super::guarded_key::KeySeed::<K>(PhantomData).deserialize(d)
    }
}

/// Custom string keys through their own `serde` impls, as in
/// [`proto_str_key_map`](super::proto_str_key_map).
struct SerdeKeys;

impl<K: Serialize + DeserializeOwned> KeyCodec<K> for SerdeKeys {
    fn serialize<S: Serializer>(key: &K, s: S) -> Result<S::Ok, S::Error> {
        key.serialize(s)
    }
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<K, D::Error> {
        K::deserialize(d)
    }
}

/// Keys through [`ProtoStringJson`], as in [`policy_str_key_map`].
struct PolicyKeys;

impl<K: ProtoStringJson> KeyCodec<K> for PolicyKeys {
    fn serialize<S: Serializer>(key: &K, s: S) -> Result<S::Ok, S::Error> {
        K::serialize_json(key.as_ref(), s)
    }
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<K, D::Error> {
        K::deserialize_json(d)
    }
}

/// `bytes` keys as base64, as in [`bytes_key_map`](super::bytes_key_map),
/// decoded once with [`bytes::decode_into`](super::bytes::decode_into), whose
/// errors never echo the key. With the `zeroize` feature a partly decoded key
/// is wiped before it is released.
struct Base64Keys;

impl<K: AsRef<[u8]> + From<Vec<u8>>> KeyCodec<K> for Base64Keys {
    fn serialize<S: Serializer>(key: &K, s: S) -> Result<S::Ok, S::Error> {
        super::bytes::serialize(key.as_ref(), s)
    }
    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<K, D::Error> {
        struct Vis;
        impl serde::de::Visitor<'_> for Vis {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a base64 map key string")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Vec<u8>, E> {
                let mut out = alloc::vec![0; crate::json_helpers::bytes::max_decoded_len(v)];
                match crate::json_helpers::bytes::decode_into::<E>(v, &mut out) {
                    Ok(len) => {
                        out.truncate(len);
                        Ok(out)
                    }
                    Err(error) => {
                        #[cfg(feature = "zeroize")]
                        zeroize::Zeroize::zeroize(&mut out);
                        Err(error)
                    }
                }
            }
            // An owned key token (from `serde_json::Value`, for example) is
            // wiped after decoding, as in `bytes_key_map`.
            #[cfg(feature = "zeroize")]
            fn visit_string<E: serde::de::Error>(
                self,
                v: alloc::string::String,
            ) -> Result<Vec<u8>, E> {
                let v = zeroize::Zeroizing::new(v);
                self.visit_str(&v)
            }
        }
        d.deserialize_str(Vis).map(K::from)
    }
}

/// Serializes a borrowed key through its codec.
struct KeyJson<'a, KC, K>(&'a K, Marker<KC>);

impl<KC: KeyCodec<K>, K> Serialize for KeyJson<'_, KC, K> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        KC::serialize(self.0, s)
    }
}

/// Deserializes a key through its codec.
struct CodecKeySeed<KC, K>(Marker<(KC, K)>);

impl<'de, KC: KeyCodec<K>, K> DeserializeSeed<'de> for CodecKeySeed<KC, K> {
    type Value = K;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<K, D::Error> {
        KC::deserialize(d)
    }
}

fn serialize_map<KC, VC, C, S>(m: &C, s: S) -> Result<S::Ok, S::Error>
where
    C: MapStorage,
    KC: KeyCodec<C::Key>,
    VC: ValueCodec<C::Value>,
    S: Serializer,
{
    use serde::ser::SerializeMap as _;
    let mut map = s.serialize_map(Some(m.storage_len()))?;
    for (k, v) in m.storage_iter() {
        map.serialize_entry(
            &KeyJson::<KC, C::Key>(k, PhantomData),
            &ValueJson::<VC, C::Value>(v, PhantomData),
        )?;
    }
    map.end()
}

fn deserialize_map<'de, KC, VC, C, D>(d: D) -> Result<C, D::Error>
where
    C: MapStorage + Default,
    KC: KeyCodec<C::Key>,
    VC: ValueCodec<C::Value>,
    D: Deserializer<'de>,
{
    struct Vis<KC, VC, C>(Marker<(KC, VC, C)>);
    impl<'de, KC, VC, C> serde::de::Visitor<'de> for Vis<KC, VC, C>
    where
        C: MapStorage + Default,
        KC: KeyCodec<C::Key>,
        VC: ValueCodec<C::Value>,
    {
        type Value = C;
        fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("a JSON object with string keys, or null")
        }
        fn visit_unit<E>(self) -> Result<C, E> {
            Ok(C::default())
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<C, A::Error> {
            let mut out = C::default();
            while let Some(k) = map.next_key_seed(CodecKeySeed::<KC, C::Key>(PhantomData))? {
                // Erase the key if its value fails to decode.
                let k = crate::erasing::Erasing::new(k, C::storage_erase_key);
                let v = map.next_value_seed(ProtoElemSeed::<Elem<VC, C::Value>>(PhantomData))?;
                out.storage_insert(k.into_inner(), v.0);
            }
            Ok(out)
        }
        crate::reject_other_json_kinds!(
            bool, i64, u64, i128, u128, f64, str, string, bytes, byte_buf, seq
        );
    }
    d.deserialize_any(Vis::<KC, VC, C>(PhantomData))
}

/// With-modules for `repeated` fields whose element type implements a policy
/// trait: one for `Vec` fields and one for custom collections.
macro_rules! policy_seq_modules {
    ($vec:ident, $list:ident, $trait:ident, $codec:ty) => {
        #[doc = concat!(
            "Serde with-module for a `repeated` field whose element type implements [`",
            stringify!($trait),
            "`], stored in a `Vec`. JSON `null` deserializes to an empty list, and a `null` ",
            "element is rejected."
        )]
        #[doc(hidden)]
        pub mod $vec {
            use super::$trait;
            use alloc::vec::Vec;
            use serde::{Deserializer, Serializer};

            /// Serialize the elements through their policy.
            pub fn serialize<T: $trait, S: Serializer>(v: &[T], s: S) -> Result<S::Ok, S::Error> {
                super::serialize_seq::<$codec, T, S>(v, s)
            }

            /// Deserialize the elements through their policy.
            pub fn deserialize<'de, T, C, D>(d: D) -> Result<C, D::Error>
            where
                T: $trait,
                C: From<Vec<T>>,
                D: Deserializer<'de>,
            {
                super::deserialize_vec::<$codec, T, D>(d).map(C::from)
            }
        }

        #[doc = concat!(
            "Serde with-module for a `repeated` field whose element type implements [`",
            stringify!($trait),
            "`], stored in a custom [`ProtoList`](crate::ProtoList) collection. Elements ",
            "are pushed straight into the collection, with no intermediate `Vec`. JSON ",
            "`null` deserializes to an empty list, and a `null` element is rejected."
        )]
        #[doc(hidden)]
        pub mod $list {
            use super::$trait;
            use crate::ProtoList;
            use serde::{Deserializer, Serializer};

            /// Serialize the elements through their policy.
            pub fn serialize<T: $trait, S: Serializer>(v: &[T], s: S) -> Result<S::Ok, S::Error> {
                super::serialize_seq::<$codec, T, S>(v, s)
            }

            /// Deserialize the elements through their policy.
            pub fn deserialize<'de, T, C, D>(d: D) -> Result<C, D::Error>
            where
                T: $trait,
                C: ProtoList<T>,
                D: Deserializer<'de>,
            {
                super::deserialize_list::<$codec, T, C, D>(d)
            }
        }
    };
}

policy_seq_modules!(
    repeated_bytes_policy,
    list_bytes_policy,
    ProtoBytesJson,
    super::BytesValues
);
policy_seq_modules!(
    repeated_string_policy,
    list_string_policy,
    ProtoStringJson,
    super::StringValues
);

/// A with-module for `map` fields whose value type implements a policy trait,
/// for one shape of key.
macro_rules! policy_map_module {
    (
        $name:ident, $trait:ident, $value_codec:ty, $key_codec:ty, $keys:literal,
        [$($key_bounds:tt)*]
    ) => {
        #[doc = concat!(
            "Serde with-module for a `map` field whose value type implements [`",
            stringify!($trait),
            "`] and whose keys are ",
            $keys,
            ". JSON `null` deserializes to an empty map, and a `null` value is rejected."
        )]
        #[doc(hidden)]
        pub mod $name {
            use super::*;

            /// Serialize the map, values through their policy.
            pub fn serialize<C, S>(m: &C, s: S) -> Result<S::Ok, S::Error>
            where
                C: MapStorage,
                C::Value: $trait,
                S: Serializer,
                $($key_bounds)*
            {
                super::serialize_map::<$key_codec, $value_codec, C, S>(m, s)
            }

            /// Deserialize the map, values through their policy.
            pub fn deserialize<'de, C, D>(d: D) -> Result<C, D::Error>
            where
                C: MapStorage + Default,
                C::Value: $trait,
                D: Deserializer<'de>,
                $($key_bounds)*
            {
                super::deserialize_map::<$key_codec, $value_codec, C, D>(d)
            }
        }
    };
}

/// Generates the four key shapes for one value policy.
macro_rules! policy_map_modules {
    ($trait:ident, $value_codec:ty, $display:ident, $str_key:ident, $policy_key:ident, $bytes_key:ident) => {
        policy_map_module!(
            $display, $trait, $value_codec, super::DisplayKeys,
            "integers, `bool` or `String`, written with `Display` and read with `FromStr` (a key's `ProtoStringJson` policy is not consulted: use the `_policy_key` variant for a policy key)",
            [C::Key: Display + FromStr,]
        );
        policy_map_module!(
            $str_key, $trait, $value_codec, super::SerdeKeys,
            "a custom string type without a JSON policy, through its own `serde` impls (a key's `ProtoStringJson` policy is not consulted: use the `_policy_key` variant for a policy key)",
            [C::Key: serde::Serialize + serde::de::DeserializeOwned,]
        );
        policy_map_module!(
            $policy_key, $trait, $value_codec, super::PolicyKeys,
            "a string type that implements [`ProtoStringJson`], through its policy",
            [C::Key: ProtoStringJson,]
        );
        policy_map_module!(
            $bytes_key, $trait, $value_codec, super::Base64Keys,
            "`string` keys normalized to bytes (`Vec<u8>` or `Bytes`), as base64",
            [C::Key: AsRef<[u8]> + From<alloc::vec::Vec<u8>>,]
        );
    };
}

policy_map_modules!(
    ProtoBytesJson,
    super::BytesValues,
    map_bytes_policy,
    map_bytes_policy_serde_key,
    map_bytes_policy_policy_key,
    map_bytes_policy_bytes_key
);
policy_map_modules!(
    ProtoStringJson,
    super::StringValues,
    map_string_policy,
    map_string_policy_serde_key,
    map_string_policy_policy_key,
    map_string_policy_bytes_key
);

/// Serde with-module for a `map` field whose `string` key type implements
/// [`ProtoStringJson`]. Keys go through the key type's policy and values
/// through [`ProtoElemJson`](super::ProtoElemJson); JSON `null` deserializes
/// to an empty map, and a `null` value is rejected as in
/// [`proto_map`](super::proto_map). When the value is also a policy type, use
/// `map_bytes_policy_policy_key` or `map_string_policy_policy_key` instead.
#[doc(hidden)]
pub mod policy_str_key_map {
    use super::{ProtoStringJson, StringPolicyJson, StringPolicySeed};
    use crate::json_helpers::{ProtoElemJson, ProtoElemSeed, ProtoJson};
    use crate::map_codec::MapStorage;
    use serde::{Deserializer, Serializer};

    /// Serialize the map, keys through their policy.
    pub fn serialize<C, S>(m: &C, s: S) -> Result<S::Ok, S::Error>
    where
        C: MapStorage,
        C::Key: ProtoStringJson,
        C::Value: ProtoElemJson,
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(m.storage_len()))?;
        for (k, v) in m.storage_iter() {
            map.serialize_entry(&StringPolicyJson::<C::Key>::new(k.as_ref()), &ProtoJson(v))?;
        }
        map.end()
    }

    /// Deserialize the map, keys through their policy.
    pub fn deserialize<'de, C, D>(d: D) -> Result<C, D::Error>
    where
        C: MapStorage + Default,
        C::Key: ProtoStringJson,
        C::Value: ProtoElemJson,
        D: Deserializer<'de>,
    {
        struct Vis<C>(core::marker::PhantomData<C>);
        impl<'de, C> serde::de::Visitor<'de> for Vis<C>
        where
            C: MapStorage + Default,
            C::Key: ProtoStringJson,
            C::Value: ProtoElemJson,
        {
            type Value = C;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a JSON object with string keys, or null")
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(C::default())
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = C::default();
                while let Some(k) = map.next_key_seed(StringPolicySeed::<C::Key>::new())? {
                    let k = crate::erasing::Erasing::new(k, C::storage_erase_key);
                    let v =
                        map.next_value_seed(ProtoElemSeed::<C::Value>(core::marker::PhantomData))?;
                    out.storage_insert(k.into_inner(), v);
                }
                Ok(out)
            }
            crate::reject_other_json_kinds!(
                bool, i64, u64, i128, u128, f64, str, string, bytes, byte_buf, seq
            );
        }
        d.deserialize_any(Vis::<C>(core::marker::PhantomData))
    }
}

#[cfg(test)]
mod tests;
