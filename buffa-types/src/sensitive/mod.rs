// Modified by ReallyMe Technologies Limited.
//! Wipe-owning alternatives for secret-bearing well-known-type fields.
//!
//! Select these types with extern-path mappings for `Timestamp`, `Duration`,
//! `Value`, `Struct` and `ListValue`. The ordinary WKT family is unchanged.
//! Strings own and wipe their allocations; numeric fields and oneof payloads
//! are wiped on drop. Dynamic collections also wipe outgrown storage.

mod repr;
pub use repr::SecretString;
// The generator deliberately spells enum defaults explicitly.
#[allow(clippy::derivable_impls)]
mod generated {
    include!("generated/google.protobuf.mod.rs");
}
pub use generated::*;
#[cfg(feature = "json")]
mod json;

impl Value {
    /// A null dynamic value.
    pub fn null() -> Self {
        Self {
            kind: Some(__buffa::oneof::value::Kind::NullValue(
                buffa::EnumValue::from(0),
            )),
        }
    }
    /// A numeric dynamic value. JSON export rejects nonfinite numbers.
    pub fn number(value: f64) -> Self {
        Self {
            kind: Some(__buffa::oneof::value::Kind::NumberValue(value)),
        }
    }
    /// Read a numeric dynamic value.
    pub fn as_number(&self) -> Option<f64> {
        match &self.kind {
            Some(__buffa::oneof::value::Kind::NumberValue(value)) => Some(*value),
            _ => None,
        }
    }
    /// Borrow a string dynamic value.
    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Some(__buffa::oneof::value::Kind::StringValue(value)) => Some(value),
            _ => None,
        }
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::number(value)
    }
}
impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Self::number(f64::from(value))
    }
}
impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self {
            kind: Some(__buffa::oneof::value::Kind::BoolValue(value)),
        }
    }
}
impl From<SecretString> for Value {
    fn from(value: SecretString) -> Self {
        Self {
            kind: Some(__buffa::oneof::value::Kind::StringValue(value)),
        }
    }
}
impl From<alloc::string::String> for Value {
    fn from(value: alloc::string::String) -> Self {
        SecretString::from(value).into()
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        SecretString::from(value).into()
    }
}
impl From<Struct> for Value {
    fn from(value: Struct) -> Self {
        Self {
            kind: Some(__buffa::oneof::value::Kind::StructValue(
                alloc::boxed::Box::new(value),
            )),
        }
    }
}
impl From<ListValue> for Value {
    fn from(value: ListValue) -> Self {
        Self {
            kind: Some(__buffa::oneof::value::Kind::ListValue(
                alloc::boxed::Box::new(value),
            )),
        }
    }
}
impl ListValue {
    /// Append a value directly into wipe-owning collection storage.
    pub fn push(&mut self, value: impl Into<Value>) {
        self.values.push(value.into());
    }
}
impl Struct {
    /// Insert a field, wiping any replaced value before release.
    pub fn insert(&mut self, key: impl Into<SecretString>, value: impl Into<Value>) {
        self.fields.insert(key.into(), value.into());
    }
}
impl Timestamp {
    /// Construct a timestamp at a whole Unix second. Wire validation remains caller-controlled.
    pub fn from_unix_secs(seconds: i64) -> Self {
        Self { seconds, nanos: 0 }
    }
}
impl Duration {
    /// Construct a whole-second duration. Wire validation remains caller-controlled.
    pub fn from_secs(seconds: i64) -> Self {
        Self { seconds, nanos: 0 }
    }
}
#[cfg(test)]
mod tests;
