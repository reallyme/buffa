//! Observe rejected transferred buffers before their allocator releases them.
#![cfg(all(feature = "zeroize", feature = "json", feature = "reflect"))]
#[path = "../../buffa/tests/support/owned_numeric.rs"]
mod owned_numeric;
use buffa_descriptor::{DescriptorPool, DynamicMessageSeed};
use buffa_types::google::protobuf as wkt;
use serde::de::{DeserializeOwned, DeserializeSeed};
use std::sync::Arc;

fn typed<T: DeserializeOwned>(text: bool) {
    if text {
        owned_numeric::observe("private-input-token", false, |value| {
            let decoder =
                serde::de::value::StringDeserializer::<serde::de::value::Error>::new(value);
            T::deserialize(decoder).is_ok()
        });
    }
    owned_numeric::observe_bytes(|value| T::deserialize(OwnedBytes(value)).is_err());
}

#[test]
fn transferred_buffers_are_wiped_at_typed_and_reflective_boundaries() {
    owned_numeric::assert_detects_unwiped_drop();
    typed::<wkt::Empty>(true);
    typed::<wkt::Struct>(true);
    typed::<wkt::ListValue>(true);
    typed::<wkt::Value>(false);
    typed::<wkt::Timestamp>(true);
    typed::<wkt::Duration>(true);
    typed::<wkt::FieldMask>(true);

    let pool = Arc::new(
        DescriptorPool::decode(include_bytes!(
            "../../buffa-descriptor/tests/protos/json_wrong_kind_test.fds"
        ))
        .unwrap(),
    );
    for name in [
        "Empty",
        "Struct",
        "ListValue",
        "Value",
        "Timestamp",
        "Duration",
        "FieldMask",
    ] {
        let index = pool
            .message_index(&format!("google.protobuf.{name}"))
            .unwrap();
        if name != "Value" {
            owned_numeric::observe("private-input-token", false, |value| {
                let decoder =
                    serde::de::value::StringDeserializer::<serde::de::value::Error>::new(value);
                DynamicMessageSeed::new(Arc::clone(&pool), index)
                    .deserialize(decoder)
                    .is_ok()
            });
        }
        owned_numeric::observe_bytes(|value| {
            DynamicMessageSeed::new(Arc::clone(&pool), index)
                .deserialize(OwnedBytes(value))
                .is_err()
        });
    }
    let pool = Arc::new(
        DescriptorPool::decode(include_bytes!(
            "../../buffa-descriptor/tests/protos/owned_input_test.fds"
        ))
        .unwrap(),
    );
    let index = pool.message_index("reflect.owned_input.Record").unwrap();
    for (field, token, accepted) in [
        ("flag", "private-input-token", false),
        ("count", "17", true),
        ("count", "private-input-token", false),
        ("state", "READY", true),
        ("state", "private-input-token", false),
        ("states", "private-input-token", false),
        ("by_state", "private-input-token", false),
    ] {
        owned_numeric::observe(token, accepted, |text| {
            let mut fields = serde_json::Map::new();
            fields.insert(field.into(), serde_json::Value::String(text));
            DynamicMessageSeed::new(Arc::clone(&pool), index)
                .deserialize(serde_json::Value::Object(fields))
                .is_ok()
        });
    }
    owned_numeric::observe_bytes(|bytes| {
        let decoder =
            serde::de::value::MapDeserializer::new(std::iter::once(("state", OwnedBytes(bytes))));
        DynamicMessageSeed::new(Arc::clone(&pool), index)
            .deserialize(decoder)
            .is_err()
    });
    let text = String::from("retained ordinary payload");
    let pointer = text.as_ptr();
    let mut fields = serde_json::Map::new();
    fields.insert("text".into(), serde_json::Value::String(text));
    let message = DynamicMessageSeed::new(pool, index)
        .deserialize(serde_json::Value::Object(fields))
        .unwrap();
    match message.field_by_number(3).unwrap() {
        buffa_descriptor::reflect::Value::String(text) => assert_eq!(text.as_ptr(), pointer),
        _ => panic!("accepted text changed representation"),
    }
}

struct OwnedBytes(Vec<u8>);
impl<'de> serde::Deserializer<'de> for OwnedBytes {
    type Error = serde::de::value::Error;
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_byte_buf(self.0)
    }
    fn deserialize_option<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_some(self)
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }
}

impl<'de> serde::de::IntoDeserializer<'de, serde::de::value::Error> for OwnedBytes {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}
