//! Construction without ordinary growable scratch buffers or partial aggregates.

use super::{Zeroize, ZeroizeOnDrop};

/// Depth-bounded arbitrary construction of a wipe-owning message or oneof.
///
/// Implementations create their cleanup owner before generating fields and
/// forward the reduced depth to nested construction. Collections receive each
/// element directly. The normal `Arbitrary` entry point starts at Buffa's
/// recursion limit; this trait also permits a tighter test-specific bound.
pub trait GuardedArbitrary<'a>: Zeroize + ZeroizeOnDrop + Sized {
    /// Construct a value while retaining cleanup on every error path.
    ///
    /// # Errors
    /// Returns `NotEnoughData` when the depth is exhausted, or an arbitrary
    /// decoding error. Custom field representations may reject generated input.
    fn arbitrary_guarded(
        input: &mut arbitrary::Unstructured<'a>,
        remaining_depth: u32,
    ) -> arbitrary::Result<Self>;
}

/// Construct bytes from caller-owned arbitrary input without staging a Vec.
#[doc(hidden)]
pub fn bytes<'a, T: crate::ProtoBytes>(
    input: &mut arbitrary::Unstructured<'a>,
) -> arbitrary::Result<T> {
    let value: &'a [u8] = input.arbitrary()?;
    T::from_wire(crate::WirePayload::borrowed(value)).map_err(|_| arbitrary::Error::IncorrectFormat)
}

/// Construct text from caller-owned arbitrary input without staging a String.
#[doc(hidden)]
pub fn string<'a, T: crate::ProtoString>(
    input: &mut arbitrary::Unstructured<'a>,
) -> arbitrary::Result<T> {
    let value: &'a str = input.arbitrary()?;
    T::from_wire(crate::WirePayload::borrowed(value.as_bytes()))
        .map_err(|_| arbitrary::Error::IncorrectFormat)
}
