// Modified by ReallyMe Technologies Limited.
//! Inspect selected allocations immediately before the system allocator frees
//! them. Reading freed pointers would be undefined behavior; observing in
//! dealloc instead verifies the actual release path, including spare capacity.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use buffa::{bytes::Buf, DecodeError, DecodeOptions, Message};
use buffa_test::guarded::{
    credentials::Credential, Credentials, CredentialsView, DirectCredentials,
};
use buffa_test::guarded_repr::{SecretBytes, SecretString};

struct ObservedAllocator;

struct WatchedAllocation {
    pointer: AtomicPtr<u8>,
    length: AtomicUsize,
    offset: AtomicUsize,
    stride: AtomicUsize,
    count: AtomicUsize,
}
impl WatchedAllocation {
    const fn new() -> Self {
        Self {
            pointer: AtomicPtr::new(std::ptr::null_mut()),
            length: AtomicUsize::new(0),
            offset: AtomicUsize::new(0),
            stride: AtomicUsize::new(0),
            count: AtomicUsize::new(1),
        }
    }
}

// The allocator hook cannot allocate its own ledger. Exhaustion is a test
// failure, so increasing the number of live buffers cannot silently hide one.
const OBSERVATION_SLOTS: usize = 8;
static WATCHED: [WatchedAllocation; OBSERVATION_SLOTS] = [
    WatchedAllocation::new(),
    WatchedAllocation::new(),
    WatchedAllocation::new(),
    WatchedAllocation::new(),
    WatchedAllocation::new(),
    WatchedAllocation::new(),
    WatchedAllocation::new(),
    WatchedAllocation::new(),
];
static ALLOCATION_SIZE: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_ALIGNMENT: AtomicUsize = AtomicUsize::new(1);
static OBSERVED: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicUsize = AtomicUsize::new(0);
static ALL_WIPED: AtomicBool = AtomicBool::new(true);
static OVERFLOWED: AtomicBool = AtomicBool::new(false);
static REALLOCATED: AtomicBool = AtomicBool::new(false);

// SAFETY: layouts and releases are forwarded unchanged to System. This
// observer is specific to the u8/str and f32 buffers in these fixtures; no typed
// values with padding are moved into watched blocks. Matched allocations are fully
// initialized, including spare capacity, and explicit watches cover buffers
// initialized beforehand. Reallocations involving a watched block or target
// size fail the observation without inspecting potentially uninitialized
// copied capacity. No allocation occurs in these hooks.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the caller's valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if layout.align() == ALLOCATION_ALIGNMENT.load(Ordering::SeqCst)
            && layout.size() == ALLOCATION_SIZE.load(Ordering::SeqCst)
            && !pointer.is_null()
        {
            // SAFETY: pointer owns layout.size() bytes. Initializing them does
            // not change GlobalAlloc's contract and avoids reading undefined
            // spare capacity when this allocation is later inspected.
            unsafe { pointer.write_bytes(0, layout.size()) };
            for slot in &WATCHED {
                if slot
                    .pointer
                    .compare_exchange(
                        std::ptr::null_mut(),
                        pointer,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    // The new pointer cannot be released before alloc returns.
                    slot.offset.store(0, Ordering::SeqCst);
                    slot.stride.store(0, Ordering::SeqCst);
                    slot.count.store(1, Ordering::SeqCst);
                    slot.length.store(layout.size(), Ordering::SeqCst);
                    OBSERVED.fetch_add(1, Ordering::SeqCst);
                    return pointer;
                }
            }
            OVERFLOWED.store(true, Ordering::SeqCst);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // System's realloc can copy uninitialized spare capacity from an
        // unobserved old block. Reject that topology instead of inspecting it.
        if layout.align() == ALLOCATION_ALIGNMENT.load(Ordering::SeqCst)
            && new_size == ALLOCATION_SIZE.load(Ordering::SeqCst)
        {
            REALLOCATED.store(true, Ordering::SeqCst);
        }
        for slot in &WATCHED {
            if slot
                .pointer
                .compare_exchange(
                    pointer,
                    std::ptr::null_mut(),
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            {
                REALLOCATED.store(true, Ordering::SeqCst);
                break;
            }
        }
        // SAFETY: all arguments are forwarded from the allocator caller.
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        for slot in &WATCHED {
            if slot.pointer.load(Ordering::SeqCst) != pointer {
                continue;
            }
            // Read length before freeing the slot for reuse by an allocation
            // on another thread. The current pointer is still live here.
            let length = slot.length.load(Ordering::SeqCst);
            let offset = slot.offset.load(Ordering::SeqCst);
            let stride = slot.stride.load(Ordering::SeqCst);
            let count = slot.count.load(Ordering::SeqCst);
            if slot
                .pointer
                .compare_exchange(
                    pointer,
                    std::ptr::null_mut(),
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            {
                // SAFETY: the allocation is live and length <= layout.size().
                // Its observed bytes were initialized as described above.
                for index in 0..count {
                    let start = index
                        .checked_mul(stride)
                        .and_then(|value| offset.checked_add(value));
                    let range =
                        start.and_then(|start| start.checked_add(length).map(|end| (start, end)));
                    if let Some((start, end)) = range.filter(|(_, end)| *end <= layout.size()) {
                        // SAFETY: this range was registered from initialized
                        // field references and is within the live allocation.
                        let bytes =
                            unsafe { std::slice::from_raw_parts(pointer.add(start), end - start) };
                        ALL_WIPED.fetch_and(bytes.iter().all(|byte| *byte == 0), Ordering::SeqCst);
                    } else {
                        ALL_WIPED.store(false, Ordering::SeqCst);
                    }
                }
                RELEASED.fetch_add(1, Ordering::SeqCst);
                break;
            }
        }
        // SAFETY: pointer and layout are forwarded from the allocator caller.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;
const SECRET_LENGTH: usize = 4093;

fn reset_observation() {
    ALLOCATION_SIZE.store(0, Ordering::SeqCst);
    ALLOCATION_ALIGNMENT.store(1, Ordering::SeqCst);
    assert!(WATCHED
        .iter()
        .all(|slot| slot.pointer.load(Ordering::SeqCst).is_null()));
    OBSERVED.store(0, Ordering::SeqCst);
    RELEASED.store(0, Ordering::SeqCst);
    ALL_WIPED.store(true, Ordering::SeqCst);
    OVERFLOWED.store(false, Ordering::SeqCst);
    REALLOCATED.store(false, Ordering::SeqCst);
}

// Every call site initialized the full capacity before truncating or wrapping
// the buffer, so the observer can inspect even its no-longer-live elements.
fn watch(pointer: *const u8, capacity: usize) {
    reset_observation();
    WATCHED[0].offset.store(0, Ordering::SeqCst);
    WATCHED[0].stride.store(0, Ordering::SeqCst);
    WATCHED[0].count.store(1, Ordering::SeqCst);
    WATCHED[0].length.store(capacity, Ordering::SeqCst);
    WATCHED[0]
        .pointer
        .store(pointer.cast_mut(), Ordering::SeqCst);
    OBSERVED.store(1, Ordering::SeqCst);
}

fn watch_next_allocation(size: usize) {
    reset_observation();
    ALLOCATION_SIZE.store(size, Ordering::SeqCst);
}

fn finish_observation() -> bool {
    ALLOCATION_SIZE.store(0, Ordering::SeqCst);
    assert!(
        !REALLOCATED.load(Ordering::SeqCst),
        "watched buffers must not reallocate during observation"
    );
    assert!(
        !OVERFLOWED.load(Ordering::SeqCst),
        "allocation observation ledger exhausted"
    );
    assert!(
        OBSERVED.load(Ordering::SeqCst) > 0,
        "no matching allocation was observed"
    );
    assert_eq!(
        RELEASED.load(Ordering::SeqCst),
        OBSERVED.load(Ordering::SeqCst),
        "watched allocations remain live"
    );
    assert!(WATCHED
        .iter()
        .all(|slot| slot.pointer.load(Ordering::SeqCst).is_null()));
    ALL_WIPED.load(Ordering::SeqCst)
}

#[track_caller]
fn assert_wiped() {
    assert!(
        finish_observation(),
        "storage was released before zeroization"
    );
}

fn length_delimited(field: u32, payload: &[u8], wire: &mut Vec<u8>) {
    buffa::encoding::Tag::new(field, buffa::encoding::WireType::LengthDelimited).encode(wire);
    buffa::types::encode_bytes(payload, wire);
}

// Keep these observations in one test: the allocator's watch slot must never
// be shared by concurrent test cases in this integration-test executable.
#[test]
fn guarded_allocations_are_wiped_before_release_on_every_lifecycle_path() {
    // Only element borrows are mutable. Neither List nor its iterator can
    // expose allocating mutable-slice algorithms; compile-fail docs freeze
    // those boundaries. Valid mutation keeps the one backing owner intact.
    let mut list: buffa::sensitive::List<u64> = vec![0x5a5a5a5a5a5a5a5a; 20_000].into();
    watch(
        list.as_ptr().cast(),
        list.capacity() * core::mem::size_of::<u64>(),
    );
    for value in &mut list {
        *value ^= 1;
    }
    *list.get_mut(0).unwrap() = 37;
    drop(list);
    assert_wiped();

    // A serde backend can transfer an owned key instead of lending text.
    // Invalid numeric keys must not drop that text before establishing a guard.
    let private_key = "K".repeat(SECRET_LENGTH);
    let pointer = private_key.as_ptr();
    let mut object = serde_json::Map::new();
    object.insert(private_key, serde_json::Value::from(1));
    watch(pointer, SECRET_LENGTH);
    let result = buffa::json_helpers::guarded_map::deserialize::<buffa::sensitive::Map<i64, u64>, _>(
        serde_json::Value::Object(object),
    );
    assert!(result.is_err());
    drop(result);
    assert_wiped();

    // The raw bytes-key helper is outside guarded codegen, but its owned
    // base64 text still follows the opt-in scratch cleanup contract.
    for invalid_key in [false, true] {
        let key = if invalid_key {
            "%".repeat(SECRET_LENGTH)
        } else {
            "Q".repeat(SECRET_LENGTH - 1)
        };
        if !invalid_key {
            use base64::Engine as _;
            assert!(base64::engine::general_purpose::STANDARD
                .decode(&key)
                .is_ok());
        }
        let source = format!(r#"{{"{key}":[]}}"#);
        watch_next_allocation(key.len());
        let result = buffa::json_helpers::bytes_key_map::deserialize::<
            buffa::sensitive::Map<Vec<u8>, u64>,
            _,
        >(&mut serde_json::Deserializer::from_str(&source));
        assert!(result.is_err());
        drop(result);
        assert_wiped();
    }

    // Invalid enum values must be rejected before Buffa materializes an owned
    // JSON string/tree. The input and parser buffering have separate owners.
    let private = "X".repeat(SECRET_LENGTH);
    let decode_record =
        |source: &str| serde_json::from_str::<buffa_test::sensitive_test::Record>(source).map(drop);
    let decode_closed = |source: &str| {
        serde_json::from_str::<buffa_test::sensitive_test::DeclaredDefaults>(source).map(drop)
    };
    type EnumDecode = fn(&str) -> Result<(), serde_json::Error>;
    let enum_inputs: [(String, EnumDecode); 8] = [
        (
            format!("{{\"optionalStatus\":{{\"private\":\"{private}\"}}}}"),
            decode_record,
        ),
        (
            format!("{{\"statusValues\":{{\"3\":\"{private}\"}}}}"),
            decode_record,
        ),
        (
            format!("{{\"statusValues\":{{\"3\":{{\"private\":\"{private}\"}}}}}}"),
            decode_record,
        ),
        (
            format!("{{\"selectedStatus\":\"{private}\"}}"),
            decode_record,
        ),
        (
            format!("{{\"optionalState\":{{\"private\":\"{private}\"}}}}"),
            decode_closed,
        ),
        (
            format!("{{\"states\":{{\"3\":\"{private}\"}}}}"),
            decode_closed,
        ),
        (
            format!("{{\"states\":{{\"3\":{{\"private\":\"{private}\"}}}}}}"),
            decode_closed,
        ),
        (format!("{{\"state\":\"{private}\"}}"), decode_closed),
    ];
    for (source, decode) in enum_inputs {
        watch_next_allocation(SECRET_LENGTH);
        let error = decode(&source).unwrap_err();
        assert!(!error.to_string().contains(&private));
        drop(error);
        ALLOCATION_SIZE.store(0, Ordering::SeqCst);
        assert_eq!(
            OBSERVED.load(Ordering::SeqCst),
            0,
            "enum decoding staged owned input"
        );
        reset_observation();
    }
    // Deserializers may transfer an owned token too. In that case the enum
    // seed becomes its cleanup owner even when the token is rejected.
    let owned = "X".repeat(SECRET_LENGTH);
    watch(owned.as_ptr(), owned.capacity());
    let token = serde::de::value::StringDeserializer::<serde::de::value::Error>::new(owned);
    assert!(
        buffa::json_helpers::guarded_enum::optional_closed::deserialize::<
            buffa_test::sensitive_test::declared_defaults::State,
            _,
        >(token)
        .is_err()
    );
    assert_wiped();

    // Exercise both standalone enum visitors. Fill the allocation before
    // truncation so the observer can check the bytes beyond the known name.
    use buffa_test::guarded::Mode;
    fn check_owned_enum<T: serde::de::DeserializeOwned + PartialEq + core::fmt::Debug>(value: T) {
        let name = "MODE_ACTIVE";
        let mut known = "X".repeat(SECRET_LENGTH);
        known.replace_range(..name.len(), name);
        known.truncate(name.len());
        watch(known.as_ptr(), SECRET_LENGTH);
        let token = serde::de::value::StringDeserializer::<serde::de::value::Error>::new(known);
        assert_eq!(T::deserialize(token).unwrap(), value);
        assert_wiped();

        let unknown = "X".repeat(SECRET_LENGTH);
        watch(unknown.as_ptr(), SECRET_LENGTH);
        let token = serde::de::value::StringDeserializer::<serde::de::value::Error>::new(unknown);
        let error = T::deserialize(token).unwrap_err();
        assert_eq!(error.to_string(), "unknown enum name");
        assert_wiped();
    }
    check_owned_enum(Mode::MODE_ACTIVE);
    check_owned_enum(buffa::EnumValue::Known(Mode::MODE_ACTIVE));

    // Decimal object keys must go directly into numeric key slots, for both
    // primitive and message-valued maps, including a later value failure.
    for source in [
        r#"{"numericScores":{"9223372036854775807":1.5}}"#,
        r#"{"numericScores":{"9223372036854775807":[]}}"#,
        r#"{"numericMessages":{"9223372036854775807":{"first":1.5}}}"#,
        r#"{"numericMessages":{"9223372036854775807":{"first":1.5,"second":[]}}}"#,
    ] {
        watch_next_allocation("9223372036854775807".len());
        drop(serde_json::from_str::<buffa_test::sensitive_test::Record>(
            source,
        ));
        ALLOCATION_SIZE.store(0, Ordering::SeqCst);
        assert_eq!(
            OBSERVED.load(Ordering::SeqCst),
            0,
            "numeric key was staged in a String"
        );
        reset_observation();
    }

    // An unwiped release must stay visible even if another equal-sized
    // allocation is subsequently wiped correctly.
    watch_next_allocation(SECRET_LENGTH);
    let unprotected = vec![b'U'; SECRET_LENGTH];
    let protected = SecretBytes::from(vec![b'K'; SECRET_LENGTH]);
    std::hint::black_box((&unprotected, &protected));
    drop(unprotected);
    drop(protected);
    assert!(
        !finish_observation(),
        "an earlier unwiped release was masked"
    );
    assert_eq!(OBSERVED.load(Ordering::SeqCst), 2);

    // Reject reallocations both into and out of the watched size. A failed
    // observation must not try to read newly copied, uninitialized capacity.
    for length in [SECRET_LENGTH.checked_sub(1).unwrap(), SECRET_LENGTH] {
        watch_next_allocation(SECRET_LENGTH);
        let mut bytes = vec![b'R'; length];
        bytes.reserve_exact(1);
        std::hint::black_box(&bytes);
        drop(bytes);
        assert!(REALLOCATED.load(Ordering::SeqCst));
        reset_observation();
    }

    let mut raw = vec![b'K'; SECRET_LENGTH];
    let pointer = raw.as_ptr();
    let capacity = raw.capacity();
    raw.truncate(17);
    let bytes = SecretBytes::from(raw);
    watch(pointer, capacity);
    drop(bytes);
    assert_wiped();

    let mut raw = "S".repeat(SECRET_LENGTH);
    let pointer = raw.as_ptr();
    let capacity = raw.capacity();
    raw.truncate(17);
    let text = SecretString::from(raw);
    watch(pointer, capacity);
    drop(text);
    assert_wiped();

    let mut message = Credentials {
        key: vec![b'K'; SECRET_LENGTH].into(),
        ..Default::default()
    };
    watch(message.key.as_ptr(), SECRET_LENGTH);
    message.clear();
    assert_wiped();

    // A malformed duplicate must not leave the old secret or any earlier
    // field merged from this input in the caller's destination.
    let options = DecodeOptions::new().clear_on_merge_error();
    message.key = vec![b'K'; SECRET_LENGTH].into();
    watch(message.key.as_ptr(), SECRET_LENGTH);
    let mut malformed = Vec::new();
    length_delimited(2, b"partially merged", &mut malformed);
    buffa::encoding::Tag::new(1, buffa::encoding::WireType::LengthDelimited).encode(&mut malformed);
    buffa::encoding::encode_varint(5, &mut malformed);
    assert_eq!(
        options.merge_from_slice(&mut message, &malformed),
        Err(DecodeError::UnexpectedEof)
    );
    assert!(message.key.is_empty());
    assert!(message.identity.is_empty());
    assert_wiped();

    // The guarded view encodes straight into the allocation watched here.
    // Its borrowed field points into that owner, which wipes before freeing.
    let source = Credentials {
        key: b"guarded backing bytes".to_vec().into(),
        ..Default::default()
    };
    let view = buffa::sensitive::GuardedOwnedView::<CredentialsView<'static>>::from_owned(
        &source,
        &DecodeOptions::new(),
    )
    .unwrap();
    assert_eq!(view.bytes(), source.encode_to_vec());
    assert_eq!(view.to_owned_message().unwrap(), source);
    let pointer = view.bytes().as_ptr();
    let len = view.bytes().len();
    watch(pointer, len);
    drop(view);
    assert_wiped();

    let wire = source.encode_to_vec();
    let view = buffa::sensitive::GuardedOwnedView::<CredentialsView<'static>>::decode(
        &wire,
        &DecodeOptions::new(),
    )
    .unwrap();
    assert_ne!(view.bytes().as_ptr(), wire.as_ptr());
    assert_eq!(view.bytes(), wire);
    assert!(matches!(
        buffa::sensitive::GuardedOwnedView::<CredentialsView<'static>>::from_owned(
            &source,
            &DecodeOptions::new().with_max_message_size(1),
        ),
        Err(buffa::sensitive::GuardedViewError::SizeLimit)
    ));
    drop(view);

    // Decode failure after copying into the clearing owner must wipe too.
    watch_next_allocation(SECRET_LENGTH);
    let mut invalid = [0_u8; SECRET_LENGTH];
    invalid[0] = 0; // illegal field number
    let err = buffa::sensitive::GuardedOwnedView::<CredentialsView<'static>>::decode(
        &invalid,
        &DecodeOptions::new(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        buffa::sensitive::GuardedViewError::Decode(DecodeError::InvalidFieldNumber)
    );
    assert_wiped();

    // Replacement through ordinary field assignment is protected too.
    message.key = vec![b'K'; SECRET_LENGTH].into();
    watch(message.key.as_ptr(), SECRET_LENGTH);
    message.key = b"new".to_vec().into();
    assert_wiped();

    for field in [1, 2, 3, 11] {
        let mut wire = Vec::new();
        length_delimited(field, &vec![b'K'; SECRET_LENGTH], &mut wire);
        length_delimited(field, b"replacement", &mut wire);
        watch_next_allocation(SECRET_LENGTH);
        let result = Credentials::decode_from_slice(&wire).unwrap();
        assert_wiped();
        drop(result);
    }

    let mut wire = Vec::new();
    length_delimited(11, &vec![b'K'; SECRET_LENGTH], &mut wire);
    length_delimited(12, b"different variant", &mut wire);
    watch_next_allocation(SECRET_LENGTH);
    let result = Credentials::decode_from_slice(&wire).unwrap();
    assert!(matches!(result.credential, Some(Credential::Text(_))));
    assert_wiped();

    // Invalid UTF-8 spans Buf chunks. The intermediate byte allocation must
    // be wiped even though a SecretString was never successfully constructed.
    let mut wire = Vec::new();
    length_delimited(2, &vec![0xff; SECRET_LENGTH], &mut wire);
    let (first, rest) = wire.split_at(7);
    watch_next_allocation(SECRET_LENGTH);
    assert!(Credentials::decode(&mut first.chain(rest)).is_err());
    assert_wiped();

    let mut wire = Vec::new();
    length_delimited(1, &vec![b'K'; SECRET_LENGTH], &mut wire);
    wire.push(0);
    watch_next_allocation(SECRET_LENGTH);
    assert!(Credentials::decode_from_slice(&wire).is_err());
    assert_wiped();

    let mut entry = Vec::new();
    length_delimited(1, b"same", &mut entry);
    length_delimited(2, &vec![b'K'; SECRET_LENGTH], &mut entry);
    let mut wire = Vec::new();
    length_delimited(8, &entry, &mut wire);
    entry.clear();
    length_delimited(1, b"same", &mut entry);
    length_delimited(2, b"new", &mut entry);
    length_delimited(8, &entry, &mut wire);
    watch_next_allocation(SECRET_LENGTH);
    let result = Credentials::decode_from_slice(&wire).unwrap();
    assert_wiped();
    drop(result);

    // The first JSON field succeeds; a later error drops its guarded value.
    let encoded = "S0tL".repeat(1364);
    let capacity = encoded.len();
    for json in [
        format!("{{\"key\":\"{encoded}\",\"identity\":[]}}"),
        format!("{{\"token\":\"{encoded}\",\"identity\":[]}}"),
        format!("{{\"chain\":[\"{encoded}\",\"%%%%\"]}}"),
        format!("{{\"keys\":{{\"first\":\"{encoded}\",\"second\":\"%%%%\"}}}}"),
        format!("{{\"raw\":\"{encoded}\",\"identity\":[]}}"),
    ] {
        watch_next_allocation(capacity);
        assert!(serde_json::from_str::<Credentials>(&json).is_err());
        assert_wiped();
    }

    // A message without oneofs uses serde's derived visitor instead of the
    // generated oneof visitor. Both paths must reach the guarded field type.
    for json in [
        format!("{{\"key\":\"{encoded}\",\"enabled\":[]}}"),
        format!("{{\"token\":\"{encoded}\",\"enabled\":[]}}"),
    ] {
        watch_next_allocation(capacity);
        assert!(serde_json::from_str::<DirectCredentials>(&json).is_err());
        assert_wiped();
    }

    // A malformed final base64 quantum fails after earlier output was written.
    let json = format!("{{\"key\":\"{encoded}%%%%\"}}");
    watch_next_allocation(capacity.checked_add(4).unwrap());
    assert!(serde_json::from_str::<Credentials>(&json).is_err());
    assert_wiped();
    // Native Serialize must run in every shape too: its base64 scratch string
    // is a separate allocation from the caller-owned serializer output.
    for field in [1, 3, 4, 8, 11] {
        let bytes = || SecretBytes::from(vec![b'K'; SECRET_LENGTH]);
        let mut value = Credentials::default();
        match field {
            1 => value.key = bytes(),
            3 => value.token = Some(bytes()),
            4 => value.chain.push(bytes()),
            8 => {
                value.keys.insert("name".to_owned().into(), bytes());
            }
            11 => value.credential = Some(Credential::Raw(bytes())),
            _ => unreachable!("fixed fixture fields"),
        }
        let encoded_length = SECRET_LENGTH
            .checked_add(2)
            .unwrap()
            .checked_div(3)
            .unwrap()
            .checked_mul(4)
            .unwrap();
        watch_next_allocation(encoded_length);
        let output = serde_json::to_string(&value).unwrap();
        assert_wiped();
        drop(output);
    }
    for optional in [false, true] {
        let bytes = SecretBytes::from(vec![b'K'; SECRET_LENGTH]);
        let mut value = DirectCredentials::default();
        if optional {
            value.token = Some(bytes);
        } else {
            value.key = bytes;
        }
        let encoded_length = SECRET_LENGTH
            .checked_add(2)
            .unwrap()
            .checked_div(3)
            .unwrap()
            .checked_mul(4)
            .unwrap();
        watch_next_allocation(encoded_length);
        let output = serde_json::to_string(&value).unwrap();
        assert_wiped();
        drop(output);
    }

    // A rejected field name can itself be private data. Supply an owned key
    // directly, so the observation isolates the generated visitor's allocation
    // from JSON tokenizer scratch and input buffers.
    fn rejected_key_is_wiped<T: serde::de::DeserializeOwned>() {
        watch_next_allocation(SECRET_LENGTH);
        let key = "P".repeat(SECRET_LENGTH);
        let source = serde::de::value::MapDeserializer::<_, serde::de::value::Error>::new(
            core::iter::once((key, ())),
        );
        let result = T::deserialize(source);
        assert!(result.is_err());
        drop(result);
        assert_wiped();
    }
    rejected_key_is_wiped::<buffa_test::sensitive_test::Record>();
    rejected_key_is_wiped::<buffa_test::sensitive_test::Empty>();
    rejected_key_is_wiped::<buffa_test::sensitive_test::DeclaredDefaults>();

    // Explicit wiping on ordinary WKTs must recurse through string values,
    // list elements and object keys too. This feature promises explicit
    // cleanup; the separate guarded family supplies automatic drop cleanup.
    use buffa_types::google::protobuf::{
        ListValue as NativeList, Struct as NativeStruct, Value as NativeValue,
    };
    use zeroize::Zeroize;
    watch_next_allocation(SECRET_LENGTH);
    let mut native = NativeValue::from(NativeStruct::from_fields([
        (
            "K".repeat(SECRET_LENGTH),
            NativeValue::from("S".repeat(SECRET_LENGTH)),
        ),
        (
            "other".to_owned(),
            NativeValue::from(NativeList {
                values: vec![
                    NativeValue::from("T".repeat(SECRET_LENGTH)),
                    NativeValue::from(1.25),
                ],
                ..Default::default()
            }),
        ),
    ]));
    native.zeroize();
    assert!(native.kind.is_none());
    assert_wiped();

    // A map's numeric message stays protected when a later JSON field fails.
    // Numeric is repr(C) and consists of eight initialized eight-byte fields.
    let numeric_size = core::mem::size_of::<buffa_test::sensitive_test::Numeric>();
    assert_eq!(numeric_size, 64);
    for json in [
        r#"{"auditValues":{"private":{"first":1.25,"second":"42"}},"enabled":[]}"#,
        r#"{"auditValues":{"private":{"first":1.25},"second":[]}}"#,
    ] {
        watch_next_allocation(numeric_size);
        ALLOCATION_ALIGNMENT.store(
            core::mem::align_of::<buffa_test::sensitive_test::Numeric>(),
            Ordering::SeqCst,
        );
        assert!(serde_json::from_str::<buffa_test::sensitive_test::Record>(json).is_err());
        assert_wiped();
    }

    // A deferred malformed child fails after the numeric map has been copied
    // into the owned parent. The converted map allocation must still be wiped.
    let mut source = buffa_test::sensitive_test::Record::default();
    source.audit_values.insert(
        "private".to_owned().into(),
        buffa_test::sensitive_test::Numeric::default().with_first(1.25),
    );
    let mut wire = source.encode_to_vec();
    length_delimited(19, &[0], &mut wire);
    let lazy =
        <buffa_test::sensitive_test::RecordLazyView as buffa::LazyMessageView>::decode_lazy(&wire)
            .unwrap();
    watch_next_allocation(numeric_size);
    ALLOCATION_ALIGNMENT.store(
        core::mem::align_of::<buffa_test::sensitive_test::Numeric>(),
        Ordering::SeqCst,
    );
    assert!(buffa::LazyMessageView::to_owned_message(&lazy).is_err());
    assert_wiped();
    drop(lazy);

    // Observe only the initialized f64 payloads of a dynamic Value list.
    // Value's enum and pointer padding must never be read by this allocator.
    use buffa_types::sensitive::{__buffa::oneof::value::Kind, Value};
    let mut values = buffa::sensitive::List::<Value>::default();
    for _ in 0..8 {
        values.push(Value::number(1.25));
    }
    let old_capacity = values.capacity();
    while values.len() < old_capacity {
        values.push(Value::number(1.25));
    }
    let base = values.as_ptr().cast::<u8>();
    let Some(Kind::NumberValue(number)) = &values[0].kind else {
        panic!("numeric fixture")
    };
    // SAFETY: both pointers refer to the same initialized Vec allocation.
    let offset =
        usize::try_from(unsafe { (number as *const f64).cast::<u8>().offset_from(base) }).unwrap();
    watch(base, core::mem::size_of::<f64>());
    WATCHED[0].offset.store(offset, Ordering::SeqCst);
    WATCHED[0]
        .stride
        .store(core::mem::size_of::<Value>(), Ordering::SeqCst);
    WATCHED[0].count.store(values.len(), Ordering::SeqCst);
    values.push(Value::number(2.5));
    assert!(values.capacity() > old_capacity);
    assert_wiped();
    let base = values.as_ptr().cast::<u8>();
    watch(base, core::mem::size_of::<f64>());
    WATCHED[0].offset.store(offset, Ordering::SeqCst);
    WATCHED[0]
        .stride
        .store(core::mem::size_of::<Value>(), Ordering::SeqCst);
    WATCHED[0].count.store(values.len(), Ordering::SeqCst);
    drop(values);
    assert_wiped();

    // Consuming an iterator transfers its yielded value, while wiping every
    // consumed and abandoned slot before the shared backing buffer is freed.
    let values: buffa::sensitive::List<f32> = (0..20).map(|_| 1.25).collect();
    watch(values.as_ptr().cast::<u8>(), core::mem::size_of::<f32>());
    WATCHED[0]
        .stride
        .store(core::mem::size_of::<f32>(), Ordering::SeqCst);
    WATCHED[0].count.store(values.len(), Ordering::SeqCst);
    let mut iter = values.into_iter();
    assert_eq!(iter.next(), Some(1.25));
    drop(iter);
    assert_wiped();

    // Observe a root view's inline handle in its still-live Box allocation.
    // Read only the initialized u64 field, never neighboring struct padding.
    let wire = buffa_test::sensitive_test::Record::default()
        .with_handle(42)
        .encode_to_vec();
    let boxed = Box::new(
        <buffa_test::sensitive_test::RecordView as buffa::MessageView>::decode_view(&wire).unwrap(),
    );
    let base = core::ptr::addr_of!(*boxed).cast::<u8>();
    let field = core::ptr::addr_of!(boxed.handle).cast::<u8>();
    // SAFETY: the field and root pointers lie in the same Box allocation.
    let offset = usize::try_from(unsafe { field.offset_from(base) }).unwrap();
    watch(base, core::mem::size_of::<u64>());
    WATCHED[0].offset.store(offset, Ordering::SeqCst);
    drop(boxed);
    assert_wiped();

    // Inspection returns scalar owners too: observe only their initialized
    // numeric payloads, never the padding of the surrounding enums.
    use buffa_descriptor::reflect::{InspectMapKeyRef, InspectValueRef, MapKeyRef};
    let boxed = Box::new(InspectValueRef::U64(37));
    let base = core::ptr::addr_of!(*boxed).cast::<u8>();
    let field = match &*boxed {
        InspectValueRef::U64(value) => core::ptr::addr_of!(*value).cast::<u8>(),
        _ => unreachable!("constructed numeric inspection slot"),
    };
    // SAFETY: both pointers belong to this live Box allocation.
    let offset = usize::try_from(unsafe { field.offset_from(base) }).unwrap();
    watch(base, core::mem::size_of::<u64>());
    WATCHED[0].offset.store(offset, Ordering::SeqCst);
    drop(boxed);
    assert_wiped();

    let boxed = Box::new(InspectMapKeyRef::from(MapKeyRef::U64(37)));
    let base = core::ptr::addr_of!(*boxed).cast::<u8>();
    let field = match boxed.value() {
        MapKeyRef::U64(value) => core::ptr::addr_of!(*value).cast::<u8>(),
        _ => unreachable!("constructed numeric inspection key"),
    };
    // SAFETY: both pointers belong to this live Box allocation.
    let offset = usize::try_from(unsafe { field.offset_from(base) }).unwrap();
    watch(base, core::mem::size_of::<u64>());
    WATCHED[0].offset.store(offset, Ordering::SeqCst);
    drop(boxed);
    assert_wiped();

    // A view owns decoded scalar buffers even while its byte fields borrow.
    // Multiple unpacked occurrences force growth, and the final malformed tag
    // exercises cleanup of a partially decoded view.
    use buffa::MessageView;
    let mut wire = Vec::new();
    for _ in 0..100 {
        buffa::encoding::Tag::new(9, buffa::encoding::WireType::Fixed32).encode(&mut wire);
        wire.extend_from_slice(&1.25f32.to_le_bytes());
    }
    for invalid in [false, true] {
        if invalid {
            wire.push(0);
        }
        watch_next_allocation(64usize.checked_mul(core::mem::size_of::<f32>()).unwrap());
        ALLOCATION_ALIGNMENT.store(core::mem::align_of::<f32>(), Ordering::SeqCst);
        let view = buffa_test::sensitive_test::RecordView::decode_view(&wire);
        assert_eq!(view.is_err(), invalid);
        drop(view);
        assert_wiped();
    }

    #[cfg(feature = "arbitrary")]
    {
        use buffa::sensitive::GuardedArbitrary;
        for invalid in [false, true] {
            let mut input = Vec::new();
            for _ in 0..100 {
                input.push(1); // Continue the repeated field.
                input.extend_from_slice(&1.25f32.to_le_bytes());
            }
            input.push(0); // Finish the repeated field.
            input.push(u8::from(invalid)); // Exceed the child depth when set.
            watch_next_allocation(64usize.checked_mul(core::mem::size_of::<f32>()).unwrap());
            ALLOCATION_ALIGNMENT.store(core::mem::align_of::<f32>(), Ordering::SeqCst);
            let result = buffa_test::sensitive_test::ArbitraryValues::arbitrary_guarded(
                &mut arbitrary::Unstructured::new(&input),
                1,
            );
            assert_eq!(result.is_err(), invalid);
            drop(result);
            assert_wiped();
        }
    }

    // Scalar embeddings must enter their guarded collection while parsing,
    // rather than after a temporary Vec has already grown or failed. Observe
    // an intermediate capacity as well as the final buffer, including spare
    // capacity, for both successful parses and errors after valid elements.
    for count in [40, 100] {
        let elements = vec!["1.25"; count].join(",");
        for invalid in [false, true] {
            let suffix = if invalid { ",\"invalid\"" } else { "" };
            let json = format!("{{\"embedding\":[{elements}{suffix}]}}");
            let capacity_bytes = 64usize.checked_mul(core::mem::size_of::<f32>()).unwrap();
            watch_next_allocation(capacity_bytes);
            ALLOCATION_ALIGNMENT.store(core::mem::align_of::<f32>(), Ordering::SeqCst);
            let result = serde_json::from_str::<Credentials>(&json);
            assert_eq!(result.is_err(), invalid);
            drop(result);
            assert_wiped();
        }
    }
}
