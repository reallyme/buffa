// Modified by ReallyMe Technologies Limited.
//! Verify that zeroizing an ordinary `Vec` of composite elements leaves no
//! element representation behind in the buffer it later releases.
//!
//! Element destructors run after each element's own cleanup, so the slots they
//! vacate (enum discriminants, inner buffer pointers and capacities, padding)
//! must be wiped again before the allocation is released. Each watched block is
//! filled with a nonzero sentinel when it is allocated, so an all-zero block at
//! release proves that a wipe covered it. Bytes are read only inside `dealloc`,
//! while the block is still live.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use buffa::{UnknownField, UnknownFieldData, UnknownFields};
use buffa_types::google::protobuf::__buffa::oneof::value::Kind;
use buffa_types::google::protobuf::{ListValue, Struct, Value};
use zeroize::Zeroize;

const SENTINEL: u8 = 0xA5;
const SLOTS: usize = 4;

struct ObservedAllocator;

static WATCHED: [AtomicPtr<u8>; SLOTS] = [
    AtomicPtr::new(std::ptr::null_mut()),
    AtomicPtr::new(std::ptr::null_mut()),
    AtomicPtr::new(std::ptr::null_mut()),
    AtomicPtr::new(std::ptr::null_mut()),
];
static ALLOCATION_SIZE: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_ALIGNMENT: AtomicUsize = AtomicUsize::new(0);
static OBSERVED: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicUsize = AtomicUsize::new(0);
static RESIDUE: AtomicUsize = AtomicUsize::new(0);
static OVERFLOWED: AtomicBool = AtomicBool::new(false);
static REALLOCATED: AtomicBool = AtomicBool::new(false);

thread_local! {
    // Const-initialized and without a destructor, so reading it inside the
    // allocator hooks does not allocate. Only the test thread arms a capture,
    // so allocations made by the test harness on other threads are ignored.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

// SAFETY: every allocation, reallocation and release is forwarded unchanged to
// System. A captured block is filled with the sentinel right after allocation.
// Typed writes can leave padding abstractly uninitialized again, but the cleanup
// under test overwrites the whole released block with zero bytes, so on the
// passing path every byte `dealloc` reads is initialized. If that cleanup
// regresses, the volatile reads below observe whatever the block holds and the
// test fails. Reads occur only in `dealloc`, before the block is handed back to
// System. No hook allocates.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the caller's valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null()
            && ARMED.with(Cell::get)
            && layout.size() == ALLOCATION_SIZE.load(Ordering::SeqCst)
            && layout.align() == ALLOCATION_ALIGNMENT.load(Ordering::SeqCst)
        {
            // SAFETY: pointer owns layout.size() writable bytes.
            unsafe { pointer.write_bytes(SENTINEL, layout.size()) };
            if WATCHED.iter().any(|slot| {
                slot.compare_exchange(
                    std::ptr::null_mut(),
                    pointer,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            }) {
                OBSERVED.fetch_add(1, Ordering::SeqCst);
            } else {
                OVERFLOWED.store(true, Ordering::SeqCst);
            }
        }
        pointer
    }

    // The default would route through `alloc` and then zero a captured block,
    // which would erase the sentinel. Zeroed blocks are never captured.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the caller's valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if WATCHED.iter().any(|slot| {
            slot.compare_exchange(
                pointer,
                std::ptr::null_mut(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        }) {
            REALLOCATED.store(true, Ordering::SeqCst);
        }
        // SAFETY: all arguments are forwarded from the allocator caller.
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WATCHED.iter().any(|slot| {
            slot.compare_exchange(
                pointer,
                std::ptr::null_mut(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        }) {
            let residue = (0..layout.size())
                // SAFETY: the block is still live and `index` is within it.
                .filter(|index| unsafe { core::ptr::read_volatile(pointer.add(*index)) } != 0)
                .count();
            RESIDUE.fetch_add(residue, Ordering::SeqCst);
            RELEASED.fetch_add(1, Ordering::SeqCst);
        }
        // SAFETY: pointer and layout are forwarded from the allocator caller.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

/// Capture every allocation of `T`-element buffers with `capacity` slots that
/// the current thread makes while `build` runs.
fn capture<T, R>(capacity: usize, build: impl FnOnce() -> R) -> R {
    assert!(WATCHED
        .iter()
        .all(|slot| slot.load(Ordering::SeqCst).is_null()));
    OBSERVED.store(0, Ordering::SeqCst);
    RELEASED.store(0, Ordering::SeqCst);
    RESIDUE.store(0, Ordering::SeqCst);
    OVERFLOWED.store(false, Ordering::SeqCst);
    REALLOCATED.store(false, Ordering::SeqCst);
    ALLOCATION_SIZE.store(capacity * core::mem::size_of::<T>(), Ordering::SeqCst);
    ALLOCATION_ALIGNMENT.store(core::mem::align_of::<T>(), Ordering::SeqCst);
    ARMED.with(|armed| armed.set(true));
    let built = build();
    ARMED.with(|armed| armed.set(false));
    ALLOCATION_SIZE.store(0, Ordering::SeqCst);
    built
}

/// The number of captured blocks, after asserting each was released all-zero.
#[track_caller]
fn assert_released_wiped() -> usize {
    assert!(
        !OVERFLOWED.load(Ordering::SeqCst),
        "observation slots exhausted"
    );
    assert!(
        !REALLOCATED.load(Ordering::SeqCst),
        "watched buffers must not reallocate during observation"
    );
    let observed = OBSERVED.load(Ordering::SeqCst);
    assert!(observed > 0, "no matching allocation was observed");
    assert_eq!(
        RELEASED.load(Ordering::SeqCst),
        observed,
        "watched allocations remain live"
    );
    assert_eq!(
        RESIDUE.load(Ordering::SeqCst),
        0,
        "released element buffers retained nonzero bytes after zeroize"
    );
    observed
}

fn unknown(number: u32, data: UnknownFieldData) -> UnknownField {
    UnknownField { number, data }
}

// One test function: the allocator's capture state is process-wide.
#[test]
fn zeroized_vec_buffers_are_released_without_element_residue() {
    // An odd capacity keeps the watched size distinct from incidental
    // growth-sized buffers. `collect` from an array allocates exactly.
    const FIELDS: usize = 5;
    let mut fields = capture::<UnknownField, _>(FIELDS, || {
        let group: UnknownFields = [
            unknown(1, UnknownFieldData::LengthDelimited(vec![0x42; 97])),
            unknown(2, UnknownFieldData::Fixed32(0x1234_5678)),
            unknown(3, UnknownFieldData::Varint(u64::MAX)),
            unknown(4, UnknownFieldData::Fixed64(0x0123_4567_89ab_cdef)),
            unknown(5, UnknownFieldData::Fixed32(1)),
        ]
        .into_iter()
        .collect();
        [
            unknown(7, UnknownFieldData::LengthDelimited(vec![0x42; 0x1357])),
            unknown(8, UnknownFieldData::Fixed32(0xdead_beef)),
            unknown(9, UnknownFieldData::Group(group)),
            unknown(10, UnknownFieldData::Varint(0xa5a5_5a5a_dead_beef)),
            unknown(11, UnknownFieldData::LengthDelimited(vec![0x24; 13])),
        ]
        .into_iter()
        .collect::<UnknownFields>()
    });
    fields.zeroize();
    assert!(fields.is_empty());
    drop(fields);
    // The outer set and its nested group were both captured and wiped.
    assert_eq!(assert_released_wiped(), 2);

    // A mixed-kind list whose elements also carry unknown fields.
    const VALUES: usize = 7;
    let element_unknown = || {
        [
            unknown(90, UnknownFieldData::LengthDelimited(vec![0x5a; 33])),
            unknown(91, UnknownFieldData::Fixed32(0x7777_7777)),
        ]
        .into_iter()
        .collect::<UnknownFields>()
    };
    let mut values = capture::<Value, _>(VALUES, || Vec::with_capacity(VALUES));
    let mut nested = Struct::default();
    nested.fields.insert(
        "nested".into(),
        Value {
            kind: Some(Kind::NumberValue(2.5)),
            ..Default::default()
        },
    );
    for kind in [
        Kind::NumberValue(1234.5),
        Kind::StringValue("private text".repeat(9)),
        Kind::BoolValue(true),
        Kind::StructValue(Box::new(nested)),
        Kind::ListValue(Box::new(ListValue {
            values: vec![Value {
                kind: Some(Kind::BoolValue(true)),
                ..Default::default()
            }],
            ..Default::default()
        })),
    ] {
        values.push(Value {
            kind: Some(kind),
            __buffa_unknown_fields: element_unknown(),
        });
    }
    values.push(Value::default());
    assert_eq!(values.capacity(), VALUES);
    let mut list = ListValue {
        values,
        __buffa_unknown_fields: element_unknown(),
    };
    list.zeroize();
    assert!(list.values.is_empty());
    drop(list);
    assert_eq!(assert_released_wiped(), 1);
}
