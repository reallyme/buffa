//! Guarded representations and message cleanup: a guarded string or bytes
//! field whose declared length runs past the message that holds it is
//! rejected before the representation copies the payload, so a failed merge
//! keeps no bytes of the parent's next field in a guarded slot.

use std::cell::Cell;

use buffa::bytes::Buf;
use buffa::{DecodeContext, DecodeError, DecodeOptions, Message};
use buffa_test::guarded_hook_frame::{Inner, Outer};
use buffa_types::sensitive::SecretString;

/// `field` as a length-delimited field whose contents are shorter than 128.
fn ld(field: u8, contents: &[u8]) -> Vec<u8> {
    let mut out = vec![field << 3 | 2, contents.len() as u8];
    out.extend_from_slice(contents);
    out
}

/// The parent's next field, which no guarded slot inside `inner` may hold.
const AFTER: &[u8] = b"\x12\x07SECRETX";

/// An `Outer` whose `inner` holds `inner` and is followed by [`AFTER`].
fn outer_with(inner: &[u8]) -> Vec<u8> {
    let mut wire = ld(1, inner);
    wire.extend_from_slice(AFTER);
    wire
}

/// A field of `Inner` declaring 10 bytes, only the first of which is inside
/// `Inner`; the other nine are [`AFTER`].
fn overrun(field: u8) -> Vec<u8> {
    vec![field << 3 | 2, 10, b'a']
}

fn overrunning_inners() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("singular string", overrun(1)),
        ("singular bytes", overrun(2)),
        ("repeated string", overrun(3)),
        ("oneof string", overrun(5)),
        ("optional string", overrun(6)),
        ("repeated bytes", overrun(7)),
        ("map key", [&[0x22, 3][..], &overrun(1)].concat()),
        ("map string value", [&[0x22, 3][..], &overrun(2)].concat()),
        ("map bytes value", [&[0x42, 3][..], &overrun(2)].concat()),
        ("map entry", vec![0x22, 10, 0x0a, 1, b'k']),
    ]
}

fn new_ctx(limit: &Cell<usize>) -> DecodeContext<'_> {
    DecodeContext::new(buffa::RECURSION_LIMIT, limit)
}

/// Whether any guarded slot of `inner` holds bytes of [`AFTER`].
fn holds_after(inner: &Inner) -> bool {
    let leaked = |bytes: &[u8]| bytes.windows(6).any(|w| w == b"SECRET");
    leaked(inner.s.as_bytes())
        || leaked(&inner.b)
        || inner.rs.iter().any(|s| leaked(s.as_bytes()))
        || inner.rb.iter().any(|b| leaked(b))
        || inner.opt.as_ref().is_some_and(|s| leaked(s.as_bytes()))
        || inner
            .m
            .iter()
            .any(|(k, v)| leaked(k.as_bytes()) || leaked(v.as_bytes()))
        || inner.mb.iter().any(|(_, v)| leaked(v))
        || matches!(
            &inner.choice,
            Some(buffa_test::guarded_hook_frame::__buffa::oneof::inner::Choice::Os(s))
                if leaked(s.as_bytes())
        )
}

#[test]
fn a_guarded_field_past_its_message_is_rejected_and_not_kept() {
    let limit = Cell::new(buffa::DEFAULT_UNKNOWN_FIELD_LIMIT);
    for (shape, inner) in overrunning_inners() {
        let wire = outer_with(&inner);
        assert_eq!(
            Outer::decode_from_slice(&wire),
            Err(DecodeError::UnexpectedEof),
            "{shape}"
        );
        for split in 1..wire.len() {
            let (head, tail) = wire.split_at(split);
            assert_eq!(
                Outer::decode(&mut head.chain(tail)),
                Err(DecodeError::UnexpectedEof),
                "{shape}, split {split}"
            );
            // The copying path keeps no out-of-frame bytes either.
            let mut existing = Outer::default();
            assert_eq!(
                existing.merge(&mut head.chain(tail), new_ctx(&limit)),
                Err(DecodeError::UnexpectedEof),
                "{shape}, split {split}"
            );
            assert!(
                !holds_after(&existing.inner),
                "{shape}, split {split} kept bytes of the parent"
            );
        }

        // A failed merge into an existing message keeps no out-of-frame bytes.
        let mut existing = Outer::default();
        existing.inner.get_or_insert_default().s = SecretString::from("keep".to_owned());
        assert_eq!(
            existing.merge_from_slice(&wire),
            Err(DecodeError::UnexpectedEof),
            "{shape}"
        );
        assert!(
            !holds_after(&existing.inner),
            "{shape} kept bytes of the parent"
        );
        assert_eq!(&*existing.inner.s, "keep", "{shape}");
    }
}

#[test]
fn a_guarded_frame_bounds_its_nested_message() {
    // A five-byte frame whose nested message declares a million bytes, filled
    // by a guarded string the rest of the stream holds. The nested message is
    // rejected against the frame before the guarded string is copied.
    let head = [5, 0x0a, 0xc0, 0x84, 0x3d, 0x0a];
    let mut tail = vec![0xbc, 0x84, 0x3d];
    tail.resize(1_000_000 - 1, b'z');
    assert_eq!(
        DecodeOptions::new()
            .with_max_message_size(16)
            .decode_length_delimited::<Outer>(&mut head.as_slice().chain(tail.as_slice())),
        Err(DecodeError::UnexpectedEof)
    );

    // Merged into an existing message, the guarded slot keeps its value.
    let limit = Cell::new(buffa::DEFAULT_UNKNOWN_FIELD_LIMIT);
    let mut existing = Outer::default();
    existing.inner.get_or_insert_default().s = SecretString::from("keep".to_owned());
    assert_eq!(
        existing
            .merge_length_delimited(&mut head.as_slice().chain(tail.as_slice()), new_ctx(&limit)),
        Err(DecodeError::UnexpectedEof)
    );
    assert_eq!(&*existing.inner.s, "keep");
}

#[test]
fn every_guarded_shape_still_decodes_inside_a_nested_message() {
    let secret = |s: &str| SecretString::from(s.to_owned());
    let mut inner = Inner::default();
    inner.s = secret("s");
    inner.b = b"b".to_vec().into();
    inner.opt = Some(secret("opt"));
    inner.choice =
        Some(buffa_test::guarded_hook_frame::__buffa::oneof::inner::Choice::Os(secret("o")));
    inner.rs.push(secret("r1"));
    inner.rs.push(secret("r2"));
    inner.rb.push(b"rb".to_vec().into());
    inner.m.insert(secret("k"), secret("v"));
    inner.mb.insert(7, b"mb".to_vec().into());
    let mut outer = Outer::default();
    outer.after = secret("after");
    *outer.inner.get_or_insert_default() = inner;
    let wire = outer.encode_to_vec();

    assert_eq!(Outer::decode_from_slice(&wire).unwrap(), outer);
    for split in 1..wire.len() {
        let (head, tail) = wire.split_at(split);
        assert_eq!(
            Outer::decode(&mut head.chain(tail)).unwrap(),
            outer,
            "split {split}"
        );
    }

    let mut stream = Vec::new();
    outer.encode_length_delimited(&mut stream);
    outer.encode_length_delimited(&mut stream);
    let mut input = stream.as_slice();
    for _ in 0..2 {
        assert_eq!(
            DecodeOptions::new()
                .decode_length_delimited::<Outer>(&mut input)
                .unwrap(),
            outer
        );
    }
    assert!(input.is_empty());
}
