//! Parse borrowed protobuf map-key text without staging an ordinary String.
use core::{fmt, marker::PhantomData, str::FromStr};
use serde::{de::DeserializeSeed, Deserializer};

#[derive(Debug, thiserror::Error)]
enum ErrorReason {
    #[error("invalid protobuf map key")]
    Invalid,
}

pub(super) struct KeySeed<K>(pub(super) PhantomData<fn() -> K>);

impl<'de, K: FromStr> DeserializeSeed<'de> for KeySeed<K> {
    type Value = K;
    fn deserialize<D: Deserializer<'de>>(self, input: D) -> Result<K, D::Error> {
        struct Key<K>(PhantomData<fn() -> K>);
        impl<K: FromStr> serde::de::Visitor<'_> for Key<K> {
            type Value = K;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a protobuf map key string")
            }
            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<K, E> {
                // Match the ordinary map helpers' FromStr grammar, including
                // signed zero, leading zeroes and an explicit positive sign.
                K::from_str(text).map_err(|_| E::custom(ErrorReason::Invalid))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                text: alloc::string::String,
            ) -> Result<K, E> {
                let text = zeroize::Zeroizing::new(text);
                self.visit_str(&text)
            }
        }
        input.deserialize_str(Key::<K>(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use serde::de::value::{Error, StringDeserializer};

    #[test]
    fn borrowed_and_owned_tokens_use_the_same_key_grammar() {
        for (text, number) in [("-0", 0), ("01", 1), ("+1", 1), ("-37", -37)] {
            let borrowed = KeySeed::<i32>(PhantomData)
                .deserialize(&mut serde_json::Deserializer::from_str(&alloc::format!(
                    "{text:?}"
                )))
                .unwrap();
            let owned = KeySeed::<i32>(PhantomData)
                .deserialize(StringDeserializer::<Error>::new(String::from(text)))
                .unwrap();
            assert_eq!((borrowed, owned), (number, number));
        }
        let error = KeySeed::<i32>(PhantomData)
            .deserialize(StringDeserializer::<Error>::new(String::from(
                "private-key",
            )))
            .unwrap_err();
        assert_eq!(error.to_string(), "invalid protobuf map key");
    }
}
