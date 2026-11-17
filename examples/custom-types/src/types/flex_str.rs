//! `string_type_custom("crate::types::FlexStr")`

use buffa::{DecodeError, ProtoString, WirePayload};

/// An owned string with inline storage and shared long-string allocations.
///
/// flexstr 0.11 borrows in `From<&str>`. A protobuf field must outlive its
/// decoding buffer, so the explicit conversion below creates owned storage.
/// Serde forwards the same string representation through the transparent owner.
#[derive(Clone, PartialEq, Eq, Hash, Default, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct FlexStr(pub flexstr::SharedStr);
super::assert_transparent!(FlexStr, flexstr::SharedStr);

impl core::ops::Deref for FlexStr {
    type Target = str;

    fn deref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<str> for FlexStr {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<String> for FlexStr {
    fn from(value: String) -> Self {
        Self(flexstr::SharedStr::from(value).optimize())
    }
}

impl From<&str> for FlexStr {
    fn from(value: &str) -> Self {
        Self(flexstr::str::SharedStrRef::from(value).into_owned())
    }
}

impl ProtoString for FlexStr {
    fn copy_from_str(value: &str) -> Self {
        value.into()
    }

    fn from_wire(payload: WirePayload<'_>) -> Result<Self, DecodeError> {
        payload.to_str().map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_inputs_become_independent_owned_strings() {
        let value = {
            let source = String::from("inline");
            FlexStr::copy_from_str(&source)
        };
        assert_eq!(value.as_ref(), "inline");
        assert!(value.0.is_inlined());
    }

    #[test]
    fn long_owned_strings_share_storage_when_cloned() {
        let value = FlexStr::from("long".repeat(128));
        let cloned = value.clone();
        assert_eq!(value, cloned);
        assert_eq!(value.as_ptr(), cloned.as_ptr());
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<FlexStr>(&json).unwrap(), value);
    }
}
