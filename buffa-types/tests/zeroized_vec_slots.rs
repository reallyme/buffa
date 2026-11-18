//! Observe released storage using fully initialized, padding-free allocations.
//!
//! Allocation-time initialization alone cannot make a scan of arbitrary Rust
//! structs sound: subsequent typed moves can leave padding uninitialized again.
//! The watched vector therefore contains only byte arrays. Its destructor writes
//! a marker after cleanup, making the missing final wipe observable even on a
//! failing run. Production containers are checked separately without reading
//! their representation or relying on their private layout. Ordinary Struct
//! payload checks register only String byte buffers, never hash-table buckets.

#![cfg(feature = "zeroize")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use buffa::{UnknownField, UnknownFieldData, UnknownFields};
use buffa_types::google::protobuf::__buffa::oneof::value::Kind;
use buffa_types::google::protobuf::{ListValue, Struct, Value};
use zeroize::Zeroize;

struct ObservedAllocator;

static WATCHED: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static WATCHED_SIZE: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicBool = AtomicBool::new(false);
static REALLOCATED: AtomicBool = AtomicBool::new(false);
static RESIDUE: AtomicUsize = AtomicUsize::new(0);
static CLEANED: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicUsize = AtomicUsize::new(0);
static BAD_DROP: AtomicBool = AtomicBool::new(false);

// SAFETY: every operation forwards the original layout to System. Only the
// explicitly registered byte-array vector or String byte buffer is inspected,
// before deallocation. Each complete allocation is initialized before use;
// byte-array moves/destructors and String ownership transfers cannot introduce
// padding or uninitialized bytes into those buffers. This also holds when the
// cleanup under test fails. No container or String header is inspected.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplies a valid layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplies a valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if WATCHED
            .compare_exchange(
                pointer,
                std::ptr::null_mut(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            REALLOCATED.store(true, Ordering::SeqCst);
        }
        // SAFETY: arguments are forwarded unchanged from the allocator caller.
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WATCHED
            .compare_exchange(
                pointer,
                std::ptr::null_mut(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
            && layout.size() == WATCHED_SIZE.load(Ordering::SeqCst)
        {
            let residue = (0..layout.size())
                // SAFETY: the registered allocation is fully initialized,
                // including on negative controls, and is still live here.
                .filter(|index| unsafe { core::ptr::read_volatile(pointer.add(*index)) } != 0)
                .count();
            RESIDUE.store(residue, Ordering::SeqCst);
            RELEASED.store(true, Ordering::SeqCst);
        }
        // SAFETY: pointer and layout are forwarded from the allocator caller.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

#[repr(transparent)]
struct Slot([u8; 32]);

impl Zeroize for Slot {
    fn zeroize(&mut self) {
        self.0.zeroize();
        CLEANED.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if self.0 != [0; 32] {
            BAD_DROP.store(true, Ordering::SeqCst);
        }
        self.0.fill(0x7d);
        DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}

fn watched_buffer() -> Vec<Slot> {
    assert!(WATCHED.load(Ordering::SeqCst).is_null());
    RELEASED.store(false, Ordering::SeqCst);
    REALLOCATED.store(false, Ordering::SeqCst);
    RESIDUE.store(0, Ordering::SeqCst);
    CLEANED.store(0, Ordering::SeqCst);
    DROPPED.store(0, Ordering::SeqCst);
    BAD_DROP.store(false, Ordering::SeqCst);
    let mut values = Vec::<Slot>::with_capacity(7);
    let size = values
        .capacity()
        .checked_mul(core::mem::size_of::<Slot>())
        .unwrap();
    let pointer = values.as_mut_ptr().cast::<u8>();
    // SAFETY: length is zero, so this writes only MaybeUninit storage in the
    // live allocation. Slot is padding-free; every subsequent typed write and
    // drop preserves initialization of all bytes that the observer will read.
    unsafe { pointer.write_bytes(0xa5, size) };
    WATCHED_SIZE.store(size, Ordering::SeqCst);
    WATCHED.store(pointer, Ordering::SeqCst);
    values
}

fn watched_text() -> String {
    const CAPACITY: usize = 97;
    const LENGTH: usize = 33;
    assert!(WATCHED.load(Ordering::SeqCst).is_null());
    RELEASED.store(false, Ordering::SeqCst);
    REALLOCATED.store(false, Ordering::SeqCst);
    RESIDUE.store(0, Ordering::SeqCst);
    let mut bytes = vec![b'Q'; CAPACITY];
    // Fill even an allocator-granted excess capacity before watching it.
    bytes.resize(bytes.capacity(), b'Q');
    let mut text = String::from_utf8(bytes).unwrap();
    text.truncate(LENGTH);
    WATCHED_SIZE.store(text.capacity(), Ordering::SeqCst);
    WATCHED.store(text.as_ptr().cast_mut(), Ordering::SeqCst);
    text
}

enum StringPayload {
    Key,
    NestedValue,
}

fn value(kind: Kind) -> Value {
    Value {
        kind: Some(kind),
        ..Default::default()
    }
}

fn ordinary_struct(payload: StringPayload) -> Struct {
    let mut outer = Struct::default();
    match payload {
        StringPayload::Key => {
            outer
                .fields
                .insert(watched_text(), value(Kind::NumberValue(1234.5)));
        }
        StringPayload::NestedValue => {
            let list = ListValue {
                values: vec![value(Kind::StringValue(watched_text()))],
                ..Default::default()
            };
            let mut child = Struct::default();
            child
                .fields
                .insert("list".into(), value(Kind::ListValue(Box::new(list))));
            outer
                .fields
                .insert("child".into(), value(Kind::StructValue(Box::new(child))));
        }
    }
    outer
}

// Keep one observer test function: it deliberately watches one allocation.
#[test]
fn allocation_cleanup_and_negative_controls() {
    for (elements, helper) in [(2usize, false), (2, true), (0, true)] {
        let mut values = watched_buffer();
        for _ in 0..elements {
            values.push(Slot([0x42; 32]));
        }
        let pointer = values.as_ptr();
        let capacity = values.capacity();
        if helper {
            buffa::sensitive::zeroize_vec(&mut values);
        } else {
            // Deliberately omit the allocation wipe. This negative control
            // stays meaningful if a future zeroize version changes Vec again.
            for value in &mut values {
                value.zeroize();
            }
            values.clear();
        }
        assert!(values.is_empty());
        assert_eq!(values.as_ptr(), pointer);
        assert_eq!(values.capacity(), capacity);
        assert_eq!(CLEANED.load(Ordering::SeqCst), elements);
        assert_eq!(DROPPED.load(Ordering::SeqCst), elements);
        assert!(!BAD_DROP.load(Ordering::SeqCst));
        drop(values);
        assert!(RELEASED.load(Ordering::SeqCst));
        assert!(!REALLOCATED.load(Ordering::SeqCst));
        if helper {
            assert_eq!(RESIDUE.load(Ordering::SeqCst), 0);
        } else {
            assert_eq!(
                RESIDUE.load(Ordering::SeqCst),
                capacity.checked_mul(core::mem::size_of::<Slot>()).unwrap()
            );
        }
    }

    for erase in [false, true] {
        for payload in [StringPayload::Key, StringPayload::NestedValue] {
            let mut message = ordinary_struct(payload);
            let capacity = message.fields.capacity();
            if erase {
                message.zeroize();
                assert!(message.fields.is_empty());
                // Explicit cleanup keeps the ordinary public map reusable.
                // This says nothing about erasure of its retained buckets.
                assert_eq!(message.fields.capacity(), capacity);
                assert!(RELEASED.load(Ordering::SeqCst));
                message
                    .fields
                    .insert("reused".into(), value(Kind::BoolValue(true)));
            }
            drop(message);
            assert!(RELEASED.load(Ordering::SeqCst));
            assert!(!REALLOCATED.load(Ordering::SeqCst));
            if erase {
                assert_eq!(RESIDUE.load(Ordering::SeqCst), 0);
            } else {
                // Ordinary Drop is deliberately the negative control. It must
                // leave both the initialized live and spare payload bytes.
                assert_eq!(
                    RESIDUE.load(Ordering::SeqCst),
                    WATCHED_SIZE.load(Ordering::SeqCst)
                );
            }
        }
    }
}

fn unknown(number: u32, data: UnknownFieldData) -> UnknownField {
    UnknownField { number, data }
}

#[test]
fn composite_callers_clear_nested_contents_and_remain_reusable() {
    let group: UnknownFields = [
        unknown(1, UnknownFieldData::LengthDelimited(vec![0x42; 97])),
        unknown(2, UnknownFieldData::Fixed32(0x1234_5678)),
        unknown(3, UnknownFieldData::Varint(u64::MAX)),
        unknown(4, UnknownFieldData::Fixed64(0x0123_4567_89ab_cdef)),
    ]
    .into_iter()
    .collect();
    let mut fields: UnknownFields = [unknown(9, UnknownFieldData::Group(group))]
        .into_iter()
        .collect();
    fields.zeroize();
    assert!(fields.is_empty());
    fields.push(unknown(1, UnknownFieldData::Varint(7)));
    assert_eq!(fields.len(), 1);
    fields.zeroize();
    assert!(fields.is_empty());

    let mut nested = Struct::default();
    nested.fields.insert(
        "nested".into(),
        Value {
            kind: Some(Kind::NumberValue(2.5)),
            ..Default::default()
        },
    );
    let mut list = ListValue {
        values: [
            Kind::NumberValue(1234.5),
            Kind::StringValue("private text".repeat(9)),
            Kind::BoolValue(true),
            Kind::StructValue(Box::new(nested)),
            Kind::ListValue(Box::default()),
        ]
        .into_iter()
        .map(|kind| Value {
            kind: Some(kind),
            __buffa_unknown_fields: [unknown(
                90,
                UnknownFieldData::LengthDelimited(vec![0x5a; 33]),
            )]
            .into_iter()
            .collect(),
        })
        .collect(),
        ..Default::default()
    };
    let pointer = list.values.as_ptr();
    let capacity = list.values.capacity();
    list.zeroize();
    assert!(list.values.is_empty());
    assert_eq!(list.values.as_ptr(), pointer);
    assert_eq!(list.values.capacity(), capacity);
    list.values.push(Value::default());
    list.zeroize();
    assert!(list.values.is_empty());
}
