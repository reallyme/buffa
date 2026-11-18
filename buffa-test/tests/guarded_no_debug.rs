//! Guarded collection decoding must not require a secret child's Debug impl.

use buffa::sensitive::Zeroize;
use buffa::view::LazyMessageView;
use buffa::{Message, MessageView};
use buffa_test::sensitive_test::{
    NoDebugChild, NoDebugParent, NoDebugParentLazyView, NoDebugParentView,
};

fn child(token: u64) -> NoDebugChild {
    NoDebugChild { token }
}

fn populated() -> NoDebugParent {
    let mut parent = NoDebugParent::default();
    parent.children.push(child(37));
    parent.children.push(child(91));
    parent.by_id.insert(7, child(43));
    parent
}

#[test]
fn no_debug_children_keep_guarded_owned_and_borrowed_round_trips() {
    let parent = populated();
    let wire = parent.encode_to_vec();
    let owned = NoDebugParent::decode_from_slice(&wire).unwrap();
    assert!(owned == parent);
    let view = NoDebugParentView::decode_view(&wire).unwrap();
    assert!(view.to_owned_message().unwrap() == parent);
    let lazy = NoDebugParentLazyView::decode_lazy(&wire).unwrap();
    assert!(lazy.to_owned_message().unwrap() == parent);
    let json = serde_json::to_vec(&parent).unwrap();
    assert!(serde_json::from_slice::<NoDebugParent>(&json).unwrap() == parent);
    assert_eq!(format!("{:?}", parent.children), "List([REDACTED])");
    assert_eq!(format!("{:?}", parent.by_id), "Map([REDACTED])");
    let mut owned = owned;
    owned.zeroize();
    assert!(owned.children.is_empty());
    assert!(owned.by_id.is_empty());
}

#[test]
fn no_debug_children_still_reject_invalid_input() {
    // The repeated child's declared payload exceeds its enclosing message.
    assert!(NoDebugParent::decode_from_slice(&[0x0a, 3, 0x08]).is_err());
    assert!(serde_json::from_str::<NoDebugParent>(r#"{"children":[null]}"#).is_err());
    assert!(serde_json::from_str::<NoDebugParent>(r#"{"children":[{"token":"bad"}]}"#).is_err());
}
