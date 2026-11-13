//! A failing from_wire distinguishes hook dispatch from the fallback even when
//! both paths could otherwise produce identical field contents.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
struct Override<T>(T);
impl<T> From<T> for Override<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}
impl<T: core::ops::Deref> core::ops::Deref for Override<T> {
    type Target = T::Target;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<[u8]> for Override<Vec<u8>> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl AsRef<str> for Override<String> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl ProtoBytes for Override<Vec<u8>> {
    fn from_wire(_: WirePayload<'_>) -> Result<Self, DecodeError> {
        Err(DecodeError::InvalidUtf8)
    }
    fn decode(buf: &mut impl Buf) -> Result<Self, DecodeError> {
        decode_bytes(buf).map(Self)
    }
}
impl ProtoString for Override<String> {
    fn from_wire(_: WirePayload<'_>) -> Result<Self, DecodeError> {
        Err(DecodeError::InvalidUtf8)
    }
    fn decode(buf: &mut impl Buf) -> Result<Self, DecodeError> {
        decode_string(buf).map(Self)
    }
}

#[test]
fn bytes_and_strings_dispatch_to_override_and_consume_one_payload() {
    let wire = [3, b'a', b'b', b'c', 0xff];
    let mut contiguous = wire.as_slice();
    assert_eq!(
        &*decode_bytes_to::<Override<Vec<u8>>>(&mut contiguous).unwrap(),
        b"abc"
    );
    assert_eq!(contiguous, [0xff]);
    let mut contiguous = wire.as_slice();
    assert_eq!(
        &*decode_string_to::<Override<String>>(&mut contiguous).unwrap(),
        "abc"
    );
    assert_eq!(contiguous, [0xff]);
    for split in 1..wire.len() {
        let mut fragmented = wire[..split].chain(&wire[split..]);
        assert_eq!(
            &*decode_bytes_to::<Override<Vec<u8>>>(&mut fragmented).unwrap(),
            b"abc"
        );
        assert_eq!(fragmented.remaining(), 1);
        let mut fragmented = wire[..split].chain(&wire[split..]);
        assert_eq!(
            &*decode_string_to::<Override<String>>(&mut fragmented).unwrap(),
            "abc"
        );
        assert_eq!(fragmented.remaining(), 1);
    }
}

#[test]
fn overrides_propagate_invalid_input_errors() {
    for wire in [&[5, b'x'][..], &[0xff, 0xff][..]] {
        assert!(decode_bytes_to::<Override<Vec<u8>>>(&mut &*wire).is_err());
        assert!(decode_string_to::<Override<String>>(&mut &*wire).is_err());
    }
    assert_eq!(
        decode_string_to::<Override<String>>(&mut &[1, 0xff][..]),
        Err(DecodeError::InvalidUtf8)
    );
}

#[derive(Clone, Debug, Default, PartialEq)]
struct RejectDefault<const OWNED: bool>(Vec<u8>);
impl<const OWNED: bool> From<Vec<u8>> for RejectDefault<OWNED> {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}
impl<const OWNED: bool> core::ops::Deref for RejectDefault<OWNED> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
impl<const OWNED: bool> AsRef<[u8]> for RejectDefault<OWNED> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl<const OWNED: bool> ProtoBytes for RejectDefault<OWNED> {
    const PREFERS_OWNED_PAYLOAD: bool = OWNED;
    fn from_wire(_: WirePayload<'_>) -> Result<Self, DecodeError> {
        Err(DecodeError::InvalidUtf8)
    }
}

#[test]
fn default_cursor_after_representation_failure_matches_payload_ownership() {
    let wire = [3, b'a', b'b', b'c', 0xff];
    let mut borrowed = wire.as_slice();
    assert_eq!(
        decode_bytes_to::<RejectDefault<false>>(&mut borrowed),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(borrowed, &wire[1..]);

    let mut owned = wire.as_slice();
    assert_eq!(
        decode_bytes_to::<RejectDefault<true>>(&mut owned),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(owned, [0xff]);

    let mut fragmented = wire[..2].chain(&wire[2..]);
    assert_eq!(
        decode_bytes_to::<RejectDefault<false>>(&mut fragmented),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(fragmented.remaining(), 1);
    assert_eq!(fragmented.get_u8(), 0xff);

    // ProtoString's default uses the borrowed path only for contiguous input.
    let mut contiguous_text = &[1, 0xff, 0x7f][..];
    assert_eq!(
        decode_string_to::<String>(&mut contiguous_text),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(contiguous_text, [0xff, 0x7f]);
    let mut fragmented_text = [2, 0xff][..].chain(&[0xff, 0x7f][..]);
    assert_eq!(
        decode_string_to::<String>(&mut fragmented_text),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(fragmented_text.remaining(), 1);
    assert_eq!(fragmented_text.get_u8(), 0x7f);
}

#[test]
fn default_cursor_consumes_only_the_prefix_on_truncated_payload() {
    let mut bytes = &[5, b'x'][..];
    assert_eq!(
        decode_bytes_to::<RejectDefault<false>>(&mut bytes),
        Err(DecodeError::UnexpectedEof)
    );
    assert_eq!(bytes, [b'x']);
    let mut text = &[5, b'x'][..];
    assert_eq!(
        decode_string_to::<String>(&mut text),
        Err(DecodeError::UnexpectedEof)
    );
    assert_eq!(text, [b'x']);
}
