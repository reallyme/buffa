// Modified by ReallyMe Technologies Limited.
use crate::borrowed_only::{EnvelopeView, PrivateRecord, PrivateRecordLazyView, PrivateRecordView};
use crate::borrowed_only::{PublicRecord, PublicRecordOwnedView};
use buffa::{HasMessageView, LazyMessageView, Message, MessageView};

#[test]
fn borrowed_only_keeps_view_family_and_unrelated_owner() {
    let record = PrivateRecord {
        data: b"borrowed".to_vec(),
        ..Default::default()
    };
    let wire = record.encode_to_vec();
    assert_eq!(
        PrivateRecordView::decode_view(&wire).unwrap().data,
        b"borrowed"
    );
    assert_eq!(
        PrivateRecordLazyView::decode_lazy(&wire).unwrap().data,
        b"borrowed"
    );
    let _ = EnvelopeView::decode_view(&[]).unwrap();
    let _ = crate::borrowed_only::nested::PrivateView::decode_view(&[]).unwrap();

    let public: PublicRecordOwnedView = PublicRecord::decode_view_handle(wire.into()).unwrap();
    let cloned = public.clone();
    assert_eq!(cloned.data(), b"borrowed");
}
