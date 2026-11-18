// Modified by ReallyMe Technologies Limited.
use super::__buffa::oneof::value::Kind;
use super::__buffa::view::oneof::value::Kind as ViewKind;
use super::{Duration, ListValue, SecretString, Struct, Timestamp, Value};
use alloc::{borrow::ToOwned, vec::Vec};
use buffa::sensitive::{Zeroize, ZeroizeOnDrop, Zeroizing};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
#[error("invalid guarded WKT JSON representation ({self:?})")]
enum ErrorReason {
    MissingKind,
    NonFinite,
    Temporal,
    View,
}

macro_rules! temporal {
    ($name:ident,$write:ident,$parse:ident) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                // Timestamp needs at most 30 bytes; Duration needs at most 24.
                const TEMPORAL_CAPACITY: usize = 30;
                let mut output = buffa::sensitive::StackText::<TEMPORAL_CAPACITY>::default();
                buffa::json_helpers::wkt::$write(&mut output, self.seconds, self.nanos)
                    .map_err(|_| serde::ser::Error::custom(ErrorReason::Temporal))?;
                serializer.serialize_str(
                    output
                        .as_str()
                        .map_err(|_| serde::ser::Error::custom(ErrorReason::Temporal))?,
                )
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $name;
                    fn expecting(
                        &self,
                        formatter: &mut core::fmt::Formatter<'_>,
                    ) -> core::fmt::Result {
                        formatter.write_str("a protobuf temporal string")
                    }
                    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
                        let value = Zeroizing::new(
                            buffa::json_helpers::wkt::$parse(text)
                                .map_err(|_| E::custom(ErrorReason::Temporal))?,
                        );
                        Ok($name {
                            seconds: value.0,
                            nanos: value.1,
                        })
                    }
                    fn visit_string<E: serde::de::Error>(
                        self,
                        text: alloc::string::String,
                    ) -> Result<Self::Value, E> {
                        let text = Zeroizing::new(text);
                        self.visit_str(&text)
                    }
                    buffa::json_helpers::reject_other_json_kinds!(
                        bool, i64, u64, i128, u128, f64, bytes, byte_buf, unit, seq, map
                    );
                }
                // A typed request lets a self-describing parser report a
                // mismatched token, value included, without the visitor.
                if deserializer.is_human_readable() {
                    deserializer.deserialize_any(Visitor)
                } else {
                    deserializer.deserialize_str(Visitor)
                }
            }
        }
    };
}
temporal!(Timestamp, write_timestamp, parse_timestamp);
temporal!(Duration, write_duration, parse_duration);

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.kind {
            None => Err(serde::ser::Error::custom(ErrorReason::MissingKind)),
            Some(Kind::NullValue(_)) => serializer.serialize_unit(),
            Some(Kind::NumberValue(value)) if value.is_finite() => serializer.serialize_f64(*value),
            Some(Kind::NumberValue(_)) => Err(serde::ser::Error::custom(ErrorReason::NonFinite)),
            Some(Kind::BoolValue(value)) => serializer.serialize_bool(*value),
            Some(Kind::StringValue(value)) => value.serialize(serializer),
            Some(Kind::StructValue(value)) => value.serialize(serializer),
            Some(Kind::ListValue(value)) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;
            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a protobuf dynamic JSON value")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
                Ok(Value::null())
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Value, E> {
                Ok(Value {
                    kind: Some(Kind::BoolValue(value)),
                })
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Value, E> {
                if !value.is_finite() {
                    return Err(E::custom(ErrorReason::NonFinite));
                }
                Ok(Value::number(value))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Value, E> {
                // Protobuf Value stores every JSON number as a double, even
                // when the token is an integer outside exact f64 precision.
                self.visit_f64(value as f64)
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Value, E> {
                self.visit_f64(value as f64)
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Value, E> {
                Ok(Value {
                    kind: Some(Kind::StringValue(value.to_owned().into())),
                })
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: alloc::string::String,
            ) -> Result<Value, E> {
                Ok(Value {
                    kind: Some(Kind::StringValue(value.into())),
                })
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Value, A::Error> {
                let mut values = buffa::sensitive::List::default();
                while let Some(value) = sequence.next_element::<Value>()? {
                    values.push(value);
                }
                Ok(Value {
                    kind: Some(Kind::ListValue(alloc::boxed::Box::new(ListValue {
                        values,
                    }))),
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut source: A,
            ) -> Result<Value, A::Error> {
                let mut fields = buffa::sensitive::Map::default();
                while let Some(key) = source.next_key::<SecretString>()? {
                    let value = source.next_value::<Value>()?;
                    fields.insert(key, value);
                }
                Ok(Value {
                    kind: Some(Kind::StructValue(alloc::boxed::Box::new(Struct { fields }))),
                })
            }
            buffa::json_helpers::reject_other_json_kinds!(i128, u128, bytes, byte_buf);
        }
        deserializer.deserialize_any(Visitor)
    }
}

impl Serialize for Struct {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.fields.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Struct {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            fields: Deserialize::deserialize(deserializer)?,
        })
    }
}
impl Serialize for ListValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.values.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ListValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            values: Deserialize::deserialize(deserializer)?,
        })
    }
}

macro_rules! element {
    ($($name:ident),* $(,)?) => {$ (
        impl buffa::json_helpers::ProtoElemJson for $name {
            fn serialize_proto_json<S:Serializer>(value:&Self,serializer:S)->Result<S::Ok,S::Error> {value.serialize(serializer)}
            fn deserialize_proto_json<'de,D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {Self::deserialize(deserializer)}
        }
    )*};
}
element!(Timestamp, Duration, Struct, ListValue);
impl buffa::json_helpers::ProtoElemJson for Value {
    fn serialize_proto_json<S: Serializer>(value: &Self, serializer: S) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }
    fn deserialize_proto_json<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        Self::deserialize(deserializer)
    }
    fn deserialize_proto_json_null<E: serde::de::Error>() -> Result<Self, E> {
        Ok(Self::null())
    }
}

macro_rules! views {
    ($view:ident,$lazy:ident) => {
        impl Serialize for super::$view<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let value = buffa::MessageView::to_owned_message(self)
                    .map_err(|_| serde::ser::Error::custom(ErrorReason::View))?;
                value.serialize(serializer)
            }
        }
        impl Serialize for super::$lazy<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let value = buffa::LazyMessageView::to_owned_message(self)
                    .map_err(|_| serde::ser::Error::custom(ErrorReason::View))?;
                value.serialize(serializer)
            }
        }
    };
}
views!(TimestampView, TimestampLazyView);
views!(DurationView, DurationLazyView);

// Retain the representation's policy for both string values and object keys.
// Borrowing must not introduce an export path that bypasses SecretString.
struct StringRef<'a>(&'a str);

impl Serialize for StringRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        <SecretString as buffa::json_helpers::ProtoStringJson>::serialize_json(self.0, serializer)
    }
}

fn serialize_value<S: Serializer>(
    kind: Option<&ViewKind<'_>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match kind {
        None => Err(serde::ser::Error::custom(ErrorReason::MissingKind)),
        Some(ViewKind::NullValue(_)) => serializer.serialize_unit(),
        Some(ViewKind::NumberValue(value)) if value.is_finite() => serializer.serialize_f64(*value),
        Some(ViewKind::NumberValue(_)) => Err(serde::ser::Error::custom(ErrorReason::NonFinite)),
        Some(ViewKind::BoolValue(value)) => serializer.serialize_bool(*value),
        Some(ViewKind::StringValue(value)) => StringRef(value).serialize(serializer),
        Some(ViewKind::StructValue(value)) => value.serialize(serializer),
        Some(ViewKind::ListValue(value)) => value.serialize(serializer),
    }
}

impl Serialize for super::ValueView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_value(self.kind.as_ref(), serializer)
    }
}

impl Serialize for super::ValueLazyView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Value's oneof message variants are eager even in its lazy view.
        serialize_value(self.kind.as_ref(), serializer)
    }
}

struct FieldRef<'view, 'wire> {
    occurrence: usize,
    entry: &'view (&'wire str, super::ValueView<'wire>),
}

// Only ordering metadata is copied. The guarded map produced by conversion
// used sorted keys and the last value for each duplicate; wire-order iteration
// (including MapView::iter_unique) would change the previous JSON byte order.
struct OrderedFields<'view, 'wire>(Vec<FieldRef<'view, 'wire>>);

impl<'view, 'wire> OrderedFields<'view, 'wire> {
    fn new(
        fields: &'view buffa::sensitive::MapView<'wire, &'wire str, super::ValueView<'wire>>,
    ) -> Self {
        let mut entries = Vec::new();
        if entries.try_reserve_exact(fields.len()).is_err() {
            // Match the collections' infallible allocation contract, including
            // capacity overflow, without a new panic or input-bearing error.
            alloc::alloc::handle_alloc_error(alloc::alloc::Layout::new::<FieldRef<'view, 'wire>>());
        }
        entries.extend(
            fields
                .iter()
                .enumerate()
                .map(|(occurrence, entry)| FieldRef { occurrence, entry }),
        );
        entries.sort_unstable_by(|a, b| {
            a.entry
                .0
                .cmp(b.entry.0)
                .then_with(|| b.occurrence.cmp(&a.occurrence))
        });
        // Descending occurrence puts the last wire value first in each run.
        // Keep it without exporting an overwritten invalid JSON value.
        entries.dedup_by(|later, earlier| later.entry.0 == earlier.entry.0);
        Self(entries)
    }
}

impl Zeroize for OrderedFields<'_, '_> {
    fn zeroize(&mut self) {
        // End the references' lifetimes before wiping their vacated storage.
        // MaybeUninit permits wiping metadata and spare capacity without ever
        // constructing invalid references or touching the borrowed payloads.
        self.0.clear();
        self.0.spare_capacity_mut().zeroize();
    }
}

impl Drop for OrderedFields<'_, '_> {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for OrderedFields<'_, '_> {}

fn serialize_fields<S: Serializer>(
    fields: &buffa::sensitive::MapView<'_, &str, super::ValueView<'_>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // Empty and singleton maps already have the required order and uniqueness.
    if fields.len() <= 1 {
        let mut map = serializer.serialize_map(Some(fields.len()))?;
        for (key, value) in fields.iter() {
            map.serialize_entry(&StringRef(key), value)?;
        }
        return map.end();
    }
    let ordered = OrderedFields::new(fields);
    let mut map = serializer.serialize_map(Some(ordered.0.len()))?;
    for field in &ordered.0 {
        map.serialize_entry(&StringRef(field.entry.0), &field.entry.1)?;
    }
    map.end()
}

impl Serialize for super::StructView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_fields(&self.fields, serializer)
    }
}

impl Serialize for super::StructLazyView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Map values are eager views, including on StructLazyView.
        serialize_fields(&self.fields, serializer)
    }
}

impl Serialize for super::ListValueView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.values.len()))?;
        for value in self.values.iter() {
            sequence.serialize_element(value)?;
        }
        sequence.end()
    }
}

impl Serialize for super::ListValueLazyView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Previously conversion validated every deferred child before invoking
        // the serializer. Keep that error priority and zero-output-on-wire-error
        // behavior with a validation pass, before writing any JSON.
        // ValueLazyView has only eager oneof children, so decoding it completes
        // wire validation for that element under its recorded resource limits.
        for value in self.values.iter() {
            value.map_err(|_| serde::ser::Error::custom(ErrorReason::View))?;
        }
        // Each access uses a fresh copy of the recorded budgets. Re-decoding
        // immutable input preserves those limits and holds only one temporary
        // child, including when many wire entries collapse to duplicate keys.
        let mut sequence = serializer.serialize_seq(Some(self.values.len()))?;
        for value in self.values.iter() {
            let value = value.map_err(|_| serde::ser::Error::custom(ErrorReason::View))?;
            sequence.serialize_element(&value)?;
        }
        sequence.end()
    }
}
