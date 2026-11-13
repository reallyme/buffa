// Modified by ReallyMe Technologies Limited.
use super::*;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

static STAGED_CONVERSIONS: AtomicUsize = AtomicUsize::new(0);
static DROPPED_ELEMENTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, Default, PartialEq)]
struct DirectFloats(Vec<f32>);

impl Drop for DirectFloats {
    fn drop(&mut self) {
        DROPPED_ELEMENTS.fetch_add(self.0.len(), Ordering::SeqCst);
    }
}

impl From<Vec<f32>> for DirectFloats {
    fn from(values: Vec<f32>) -> Self {
        STAGED_CONVERSIONS.fetch_add(1, Ordering::SeqCst);
        Self(values)
    }
}

impl FromIterator<f32> for DirectFloats {
    fn from_iter<I: IntoIterator<Item = f32>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl core::ops::Deref for DirectFloats {
    type Target = [f32];
    fn deref(&self) -> &[f32] {
        &self.0
    }
}

impl ProtoList<f32> for DirectFloats {
    fn push(&mut self, value: f32) {
        self.0.push(value);
    }
    fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Holder(#[serde(with = "super")] DirectFloats);

#[test]
fn custom_collection_owns_elements_during_success_and_partial_failure() {
    {
        let decoded: Holder = serde_json::from_str("[1,2,3]").unwrap();
        assert_eq!(&*decoded.0, &[1.0, 2.0, 3.0]);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), "[1.0,2.0,3.0]");
    }
    for input in ["[]", "null"] {
        let decoded: Holder = serde_json::from_str(input).unwrap();
        assert!(decoded.0.is_empty());
    }

    STAGED_CONVERSIONS.store(0, Ordering::SeqCst);
    DROPPED_ELEMENTS.store(0, Ordering::SeqCst);
    assert!(serde_json::from_str::<Holder>(r#"[1,2,"invalid"]"#).is_err());
    assert_eq!(DROPPED_ELEMENTS.load(Ordering::SeqCst), 2);
    assert_eq!(STAGED_CONVERSIONS.load(Ordering::SeqCst), 0);
    assert!(serde_json::from_str::<Holder>("[null]").is_err());
    assert!(serde_json::from_str::<Holder>("{}").is_err());
}
