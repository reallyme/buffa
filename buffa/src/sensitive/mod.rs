// Modified by ReallyMe Technologies Limited.
//! Opt-in owned storage for sensitive protobuf messages.
//!
//! Collections wipe their values before releasing them. A list also wipes
//! outgrown allocations; a map keeps keys and values in stable wipe-owned
//! allocations so tree balancing only moves pointers.

#[cfg(feature = "arbitrary")]
#[doc(hidden)]
pub mod arbitrary;
mod list;
mod map;
mod owned_view;
#[cfg(feature = "arbitrary")]
pub use arbitrary::GuardedArbitrary;
pub use list::{IterMut, List};
pub use map::{Map, MapKey};
pub use owned_view::{GuardedOwnedView, GuardedViewError};
pub use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Fixed diagnostics used by generated guarded JSON visitors.
#[cfg(feature = "json")]
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JsonErrorReason {
    /// Unknown field names can contain caller-provided private data.
    #[error("unknown JSON field in guarded message")]
    UnknownField,
}

impl<T: Default + Zeroize, P: crate::ProtoBox<T>> Zeroize for crate::MessageField<T, P> {
    fn zeroize(&mut self) {
        if let Some(value) = self.as_option_mut() {
            value.zeroize();
        }
        *self = Self::none();
    }
}

/// Overwrite a copyable value with its valid default using volatile stores.
/// Used by generated enums, whose valid default is not necessarily zero.
#[doc(hidden)]
pub fn reset_copy<T: Copy + Default>(value: &mut T) {
    // Construct first: even a panicking custom Default must leave the original
    // value valid. A typed assignment alone can leave inactive enum bytes behind.
    let replacement = T::default();
    let pointer = value as *mut T;
    // SAFETY: the exclusive slot is valid storage for MaybeUninit<T>, which
    // permits zero bytes even when T does not. No T is accessed between wiping
    // the representation and restoring its valid default. Copy has no destructor
    // or owned allocation that overwriting could leak.
    unsafe {
        (&mut *pointer.cast::<core::mem::MaybeUninit<T>>()).zeroize();
        core::ptr::write_volatile(pointer, replacement);
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

impl<E: crate::Enumeration + Zeroize> Zeroize for crate::EnumValue<E> {
    fn zeroize(&mut self) {
        match self {
            Self::Unknown(number) => number.zeroize(),
            Self::Known(value) => value.zeroize(),
        }
        // Known discriminants belong to the public enum definition. Replacing
        // the whole representation also overwrites a previous identifying number.
        reset_copy(self);
    }
}

impl<T: Default + Zeroize + ZeroizeOnDrop, P: crate::ProtoBox<T>> ZeroizeOnDrop
    for crate::MessageField<T, P>
{
}

mod text;
#[cfg(feature = "json")]
pub(crate) use text::serialize_decimal;
pub use text::{FormatError, StackText};

pub(crate) mod view_storage;
#[doc(hidden)]
pub use view_storage::IntoIter as ViewIntoIter;

impl<T: Zeroize> Zeroize for crate::MessageFieldView<T> {
    fn zeroize(&mut self) {
        if let Some(value) = self.as_mut() {
            value.zeroize();
        }
        *self = Self::unset();
    }
}
impl<V> Zeroize for crate::LazyMessageFieldView<'_, V> {
    fn zeroize(&mut self) {
        *self = Self::default();
    }
}
impl<V> Zeroize for crate::LazyRepeatedView<'_, V> {
    fn zeroize(&mut self) {
        *self = Self::default();
    }
}

#[doc(hidden)]
pub use crate::erasing::Erasing as Slot;

pub use crate::view::guarded::{MapView, RepeatedView};

#[cfg(test)]
mod tests {
    use super::reset_copy;
    #[test]
    fn copy_reset_restores_valid_niche_and_tagged_representations() {
        #[derive(Clone, Copy, Debug, PartialEq)]
        struct NonZero(core::num::NonZeroU32);
        impl Default for NonZero {
            fn default() -> Self {
                Self(core::num::NonZeroU32::new(7).unwrap())
            }
        }
        let mut value = NonZero(core::num::NonZeroU32::new(37).unwrap());
        reset_copy(&mut value);
        assert_eq!(value, NonZero::default());

        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        enum Slot {
            #[default]
            Empty,
            Number([u64; 4]),
        }
        let mut value = Slot::Number([37; 4]);
        reset_copy(&mut value);
        assert_eq!(value, Slot::Empty);
        reset_copy(&mut value);
        assert_eq!(value, Slot::Empty);

        #[cfg(feature = "std")]
        {
            #[derive(Clone, Copy)]
            struct PanickingDefault(core::num::NonZeroU32);
            impl Default for PanickingDefault {
                fn default() -> Self {
                    panic!("test default failure");
                }
            }
            let mut value = PanickingDefault(core::num::NonZeroU32::new(37).unwrap());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                reset_copy(&mut value);
            }));
            assert!(result.is_err());
            assert_eq!(value.0.get(), 37);
        }
    }
}
