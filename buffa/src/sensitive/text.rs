use super::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// A bounded temporary text buffer whose entire storage is wiped on drop.
/// Formatting writes directly into the owner, without intermediate heap strings.
pub struct StackText<const N: usize> {
    bytes: Zeroizing<[u8; N]>,
    length: usize,
}
/// A bounded text-formatting failure without input data in its diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// The fixed buffer could not hold the formatted representation.
    #[error("sensitive text capacity exceeded")]
    Capacity,
    /// A writer produced invalid UTF-8.
    #[error("sensitive text is not UTF-8")]
    Utf8,
}
impl<const N: usize> Default for StackText<N> {
    fn default() -> Self {
        Self {
            bytes: Zeroizing::new([0; N]),
            length: 0,
        }
    }
}
impl<const N: usize> StackText<N> {
    /// Borrow the initialized text.
    /// # Errors
    /// Returns a fixed error if a writer produced invalid UTF-8.
    pub fn as_str(&self) -> Result<&str, FormatError> {
        let bytes = self.bytes.get(..self.length).ok_or(FormatError::Capacity)?;
        core::str::from_utf8(bytes).map_err(|_| FormatError::Utf8)
    }
}
impl<const N: usize> core::fmt::Write for StackText<N> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let end = self
            .length
            .checked_add(value.len())
            .ok_or(core::fmt::Error)?;
        let target = self
            .bytes
            .get_mut(self.length..end)
            .ok_or(core::fmt::Error)?;
        target.copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}
impl<const N: usize> Zeroize for StackText<N> {
    fn zeroize(&mut self) {
        self.bytes.zeroize();
        self.length.zeroize();
    }
}
impl<const N: usize> Drop for StackText<N> {
    fn drop(&mut self) {
        self.zeroize();
    }
}
impl<const N: usize> ZeroizeOnDrop for StackText<N> {}

#[cfg(feature = "json")]
pub(crate) fn serialize_decimal<T: core::fmt::Display, S: serde::Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use core::fmt::Write;
    // u64::MAX and i64::MIN both need at most twenty decimal characters.
    const DECIMAL_CAPACITY: usize = 20;
    let mut output = StackText::<DECIMAL_CAPACITY>::default();
    write!(&mut output, "{value}").map_err(|_| serde::ser::Error::custom(FormatError::Capacity))?;
    serializer.serialize_str(output.as_str().map_err(serde::ser::Error::custom)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;
    #[test]
    fn formatting_is_bounded_and_reusable_after_erasure() {
        let mut text = StackText::<4>::default();
        text.write_str("é").unwrap();
        assert_eq!(text.as_str().unwrap(), "é");
        assert!(text.write_str("abc").is_err());
        assert_eq!(text.as_str().unwrap(), "é");
        text.zeroize();
        assert_eq!(text.as_str().unwrap(), "");
        assert!(text.bytes.iter().all(|byte| *byte == 0));
        text.write_str("1234").unwrap();
        assert!(text.write_char('5').is_err());
        assert!(StackText::<0>::default().write_char('x').is_err());
    }
}
