//! The shared visitor must work in consumers with shadowed primitive names.

#![cfg(feature = "json")]

use serde::de::{value::Error, Visitor};

#[allow(non_camel_case_types, dead_code)]
mod consumer {
    pub struct bool;
    pub struct i64;
    pub struct u64;
    pub struct i128;
    pub struct u128;
    pub struct f64;
    pub struct str;
    pub struct u8;

    pub struct Reject;

    impl<'de> serde::de::Visitor<'de> for Reject {
        type Value = ();

        fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("a supported protobuf value")
        }

        buffa::json_helpers::reject_other_json_kinds!(
            bool, i64, u64, i128, u128, f64, str, string, bytes, byte_buf, unit, seq, map
        );
    }
}

struct Integer;

impl<'de> Visitor<'de> for Integer {
    type Value = i128;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a 64-bit integer")
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(i128::from(value))
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(i128::from(value))
    }

    buffa::json_helpers::reject_other_json_kinds!(i128, u128);
}

struct LegacyRoot;

impl<'de> Visitor<'de> for LegacyRoot {
    type Value = ();

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a supported protobuf value")
    }

    buffa::reject_other_json_kinds!(i128, u128);
}

#[test]
fn narrow_integer_visits_preserve_the_64_bit_boundary() {
    for value in [
        i128::from(i64::MIN),
        -1,
        0,
        i128::from(i64::MAX),
        i128::from(u64::MAX),
    ] {
        assert_eq!(Integer.visit_i128::<Error>(value).unwrap(), value);
    }
    for value in [0, u128::from(i64::MAX.unsigned_abs()), u128::from(u64::MAX)] {
        assert_eq!(
            Integer.visit_u128::<Error>(value).unwrap(),
            i128::try_from(value).unwrap()
        );
    }
}

#[test]
fn wide_integers_are_rejected_without_echoing_the_value() {
    for value in [
        i128::MIN,
        i128::from(i64::MIN) - 1,
        i128::from(u64::MAX) + 1,
        i128::MAX,
    ] {
        let error = Integer.visit_i128::<Error>(value).unwrap_err().to_string();
        assert!(error.contains("integer"));
        assert!(!error.contains(&value.to_string()));
    }
    for value in [u128::from(u64::MAX) + 1, u128::MAX] {
        let error = Integer.visit_u128::<Error>(value).unwrap_err().to_string();
        assert!(error.contains("integer"));
        assert!(!error.contains(&value.to_string()));
    }
}

#[test]
fn rejected_owned_values_and_forwarded_integers_remain_redacted() {
    let canary = "PRIVATE-REJECTED-TOKEN-813624";
    for error in [
        consumer::Reject
            .visit_string::<Error>(canary.to_owned())
            .unwrap_err(),
        consumer::Reject
            .visit_byte_buf::<Error>(canary.as_bytes().to_vec())
            .unwrap_err(),
        consumer::Reject.visit_i128::<Error>(813624).unwrap_err(),
        consumer::Reject.visit_u128::<Error>(813624).unwrap_err(),
    ] {
        let error = error.to_string();
        assert!(!error.contains(canary));
        assert!(!error.contains("813624"));
    }
    assert!(LegacyRoot.visit_i128::<Error>(813624).is_err());
}
