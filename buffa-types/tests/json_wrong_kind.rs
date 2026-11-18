//! Wrong-kind JSON input for a well-known type is rejected without echoing
//! the input in the error.

#![cfg(feature = "json")]

use buffa_types::google::protobuf as wkt;
use serde::de::{DeserializeOwned, IntoDeserializer};

/// Text that would appear in a diagnostic that echoes one of the inputs.
const MARKERS: [&str; 5] = [
    "123456789",
    "1.5e",
    "true",
    "marker",
    "340282366920938463463374607431768211455",
];

const INT: &str = "123456789";
const FLOAT: &str = "-1.5e300";
const BOOL: &str = "true";
const STR: &str = r#""marker-secret""#;
const SEQ: &str = "[]";
const MAP: &str = "{}";

fn assert_redacted(input: &str, text: &str) {
    for marker in MARKERS {
        assert!(
            !text.contains(marker),
            "error for {input} echoes {marker:?}: {text}"
        );
    }
}

/// Feed each JSON input, and `u128::MAX` through serde's `U128Deserializer`,
/// to `T` and assert that every error omits the input.
fn assert_wrong_kinds_redacted<T: DeserializeOwned>(inputs: &[&str]) {
    let name = core::any::type_name::<T>();
    for input in inputs {
        let err = serde_json::from_str::<T>(input).err();
        let text = err.map_or_else(|| panic!("{input} decoded as {name}"), |e| e.to_string());
        assert_redacted(input, &text);
    }
    let de = IntoDeserializer::<serde::de::value::Error>::into_deserializer(u128::MAX);
    let err = <T as serde::Deserialize>::deserialize(de).err();
    let text = err.map_or_else(|| panic!("u128::MAX decoded as {name}"), |e| e.to_string());
    assert_redacted(name, &text);
}

#[test]
fn string_forms_omit_wrong_kind_input() {
    let not_string = [INT, FLOAT, BOOL, SEQ, MAP];
    assert_wrong_kinds_redacted::<wkt::Timestamp>(&not_string);
    assert_wrong_kinds_redacted::<wkt::Duration>(&not_string);
    assert_wrong_kinds_redacted::<wkt::FieldMask>(&not_string);
    assert_wrong_kinds_redacted::<wkt::StringValue>(&not_string);
}

#[test]
fn structural_forms_omit_wrong_kind_input() {
    assert_wrong_kinds_redacted::<wkt::Struct>(&[INT, FLOAT, BOOL, STR, SEQ]);
    assert_wrong_kinds_redacted::<wkt::Empty>(&[INT, FLOAT, BOOL, STR, SEQ]);
    assert_wrong_kinds_redacted::<wkt::ListValue>(&[INT, FLOAT, BOOL, STR, MAP]);
    // `Value` accepts every JSON kind; a 128-bit integer is rejected by kind.
    assert_wrong_kinds_redacted::<wkt::Value>(&[]);
}

#[test]
fn scalar_wrappers_omit_wrong_kind_input() {
    assert_wrong_kinds_redacted::<wkt::BoolValue>(&[INT, FLOAT, STR, SEQ, MAP]);
    assert_wrong_kinds_redacted::<wkt::Int32Value>(&[BOOL, STR, SEQ, MAP]);
    assert_wrong_kinds_redacted::<wkt::UInt64Value>(&[BOOL, STR, SEQ, MAP]);
    assert_wrong_kinds_redacted::<wkt::DoubleValue>(&[BOOL, STR, SEQ, MAP]);
    assert_wrong_kinds_redacted::<wkt::BytesValue>(&[INT, FLOAT, BOOL, STR, SEQ, MAP]);
}

#[test]
fn object_forms_reject_the_number_token_key() {
    // `serde_json`'s `arbitrary_precision` feature hands a number to an
    // object visitor under this key.
    let token = r#"{"$serde_json::private::Number":"1.5"}"#;
    let err = serde_json::from_str::<wkt::Empty>(token).unwrap_err();
    assert!(err.to_string().starts_with("invalid type: number"), "{err}");
    // Other keys are still ignored.
    assert!(serde_json::from_str::<wkt::Empty>(r#"{"k":1}"#).is_ok());
}

/// A deserializer that is not human-readable and answers only typed requests:
/// an empty object, an empty array, or its string. `deserialize_any` fails.
struct Typed(&'static str);

impl<'de> serde::Deserializer<'de> for Typed {
    type Error = serde::de::value::Error;

    fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("deserialize_any"))
    }
    fn deserialize_map<V: serde::de::Visitor<'de>>(self, v: V) -> Result<V::Value, Self::Error> {
        v.visit_map(serde::de::value::MapDeserializer::<_, Self::Error>::new(
            core::iter::empty::<((), ())>(),
        ))
    }
    fn deserialize_seq<V: serde::de::Visitor<'de>>(self, v: V) -> Result<V::Value, Self::Error> {
        v.visit_seq(serde::de::value::SeqDeserializer::<_, Self::Error>::new(
            core::iter::empty::<()>(),
        ))
    }
    fn deserialize_str<V: serde::de::Visitor<'de>>(self, v: V) -> Result<V::Value, Self::Error> {
        v.visit_str(self.0)
    }
    fn deserialize_string<V: serde::de::Visitor<'de>>(self, v: V) -> Result<V::Value, Self::Error> {
        v.visit_str(self.0)
    }
    fn is_human_readable(&self) -> bool {
        false
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char bytes byte_buf
        option unit unit_struct newtype_struct tuple tuple_struct struct enum
        identifier ignored_any
    }
}

#[test]
fn non_human_readable_formats_keep_the_typed_request() {
    use serde::Deserialize;
    assert_eq!(
        wkt::Struct::deserialize(Typed("")).unwrap(),
        wkt::Struct::default()
    );
    assert_eq!(
        wkt::ListValue::deserialize(Typed("")).unwrap(),
        wkt::ListValue::default()
    );
    assert_eq!(
        wkt::Empty::deserialize(Typed("")).unwrap(),
        wkt::Empty::default()
    );
    let ts = wkt::Timestamp::deserialize(Typed("1970-01-01T00:00:01Z")).unwrap();
    assert_eq!((ts.seconds, ts.nanos), (1, 0));
    let d = wkt::Duration::deserialize(Typed("1.5s")).unwrap();
    assert_eq!((d.seconds, d.nanos), (1, 500_000_000));
    let mask = wkt::FieldMask::deserialize(Typed("fooBar")).unwrap();
    assert_eq!(mask.paths, ["foo_bar"]);
}

#[test]
fn value_accepts_in_range_128_bit_integers() {
    use serde::de::value::{Error, I128Deserializer, U128Deserializer};
    let value = <wkt::Value as serde::Deserialize>::deserialize(I128Deserializer::<Error>::new(-7));
    assert_eq!(value.unwrap(), wkt::Value::from(-7.0));
    let value = <wkt::Value as serde::Deserialize>::deserialize(U128Deserializer::<Error>::new(7));
    assert_eq!(value.unwrap(), wkt::Value::from(7.0));
}
