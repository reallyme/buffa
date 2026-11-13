use super::__buffa::oneof::value::Kind;
use super::{Duration, ListValue, SecretString, Struct, Timestamp, Value};
use alloc::borrow::ToOwned;
use buffa::sensitive::Zeroizing;
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
                impl serde::de::Visitor<'_> for Visitor {
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
                }
                deserializer.deserialize_str(Visitor)
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
    ($view:ident,$lazy:ident,$owned:ident) => {
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
        impl Serialize for super::$owned {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.view().serialize(serializer)
            }
        }
    };
}
views!(TimestampView, TimestampLazyView, TimestampOwnedView);
views!(DurationView, DurationLazyView, DurationOwnedView);
views!(ValueView, ValueLazyView, ValueOwnedView);
views!(StructView, StructLazyView, StructOwnedView);
views!(ListValueView, ListValueLazyView, ListValueOwnedView);
