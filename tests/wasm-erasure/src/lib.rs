//! Test-only allocation observations, executed natively and as WebAssembly.
//!
//! Only explicitly registered, fully initialized, padding-free allocations are
//! inspected. The observer runs before the real allocator releases the storage.
//! Wasm sends the live region to a JavaScript import, so assertions observe the
//! module's linear memory rather than relying on a Rust logical-empty check.

#![cfg(feature = "runtime-tests")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use buffa::sensitive::{zeroize_vec, List, Map, Zeroize};
use buffa_types::sensitive::{ListValue, SecretString, Struct};
use serde::Deserialize;

const MAX_WATCHES: usize = 8;
const SLOT_CAPACITY: usize = 4;
const TEXT_CAPACITY: usize = 96;
const TEXT_LENGTH: usize = 64;
const CASE_COUNT: u32 = 11;

struct Watch {
    pointer: AtomicPtr<u8>,
    length: AtomicUsize,
}

impl Watch {
    const fn new() -> Self {
        Self {
            pointer: AtomicPtr::new(std::ptr::null_mut()),
            length: AtomicUsize::new(0),
        }
    }
}

// Spell the non-Copy initializers explicitly for Rust 1.75 (inline const array
// repetition requires a newer compiler).
static WATCHES: [Watch; MAX_WATCHES] = [
    Watch::new(),
    Watch::new(),
    Watch::new(),
    Watch::new(),
    Watch::new(),
    Watch::new(),
    Watch::new(),
    Watch::new(),
];
static WATCH_COUNT: AtomicUsize = AtomicUsize::new(0);
static RELEASES: AtomicUsize = AtomicUsize::new(0);
static DIRTY_RELEASES: AtomicUsize = AtomicUsize::new(0);
static BAD_OBSERVER: AtomicBool = AtomicBool::new(false);
static CHECK_DROPS: AtomicBool = AtomicBool::new(false);
static BAD_DROP: AtomicBool = AtomicBool::new(false);
static DROPS: AtomicUsize = AtomicUsize::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "erasure_test")]
extern "C" {
    fn observe_release(pointer: *const u8, length: usize, watch: usize) -> usize;
}

/// Inspect a known initialized region while its allocation is still live.
///
/// # Safety
/// `pointer..pointer + length` must remain readable and fully initialized for
/// this call. The caller must not expose padding or a freed allocation.
#[cfg(not(target_arch = "wasm32"))]
unsafe fn observe_release(pointer: *const u8, length: usize, _watch: usize) -> usize {
    (0..length)
        // SAFETY: the caller guarantees every byte in this live region is
        // initialized, including when the cleanup being tested is defective.
        .filter(|index| unsafe { pointer.add(*index).read_volatile() } != 0)
        .count()
}

struct ObservedAllocator;

/// Observe only a previously registered allocation, before forwarding release.
///
/// # Safety
/// The pointer and layout must describe a live allocation owned by System.
unsafe fn before_release(pointer: *mut u8, layout: Layout) {
    for (index, watch) in WATCHES.iter().enumerate() {
        if watch
            .pointer
            .compare_exchange(
                pointer,
                std::ptr::null_mut(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            let length = watch.length.load(Ordering::SeqCst);
            if length != layout.size() {
                BAD_OBSERVER.store(true, Ordering::SeqCst);
                return;
            }
            // SAFETY: registration requires a fully initialized byte region
            // covering this allocation, with no padding. It is still owned
            // here. The JavaScript observer never calls back into the module.
            let dirty = unsafe { observe_release(pointer, length, index) };
            if dirty != 0 {
                DIRTY_RELEASES.fetch_add(1, Ordering::SeqCst);
            }
            RELEASES.fetch_add(1, Ordering::SeqCst);
            return;
        }
    }
}

// SAFETY: allocation and release forward System's exact pointer/layout pairs.
// Observations neither allocate in Rust nor modify the watched storage. Watches
// only contain padding-free regions whose complete initialization is established
// by the fixture, never an arbitrary Rust value's representation.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: GlobalAlloc's caller supplies the live allocation and layout.
        unsafe { before_release(pointer, layout) };
        // SAFETY: forward the original allocation unchanged, after observation.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if WATCHES
            .iter()
            .any(|watch| watch.pointer.load(Ordering::SeqCst) == pointer)
        {
            // These fixtures only permit wipe-then-release growth. Seeing a
            // watched allocation passed to realloc is a regression, even if
            // the allocator happens to grow it in place on this run.
            BAD_OBSERVER.store(true, Ordering::SeqCst);
            // SAFETY: realloc's old allocation is still live before forwarding.
            unsafe { before_release(pointer, layout) };
        }
        // SAFETY: all arguments come unchanged from GlobalAlloc's caller.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

#[derive(Debug, thiserror::Error)]
enum ErrorReason {
    #[error("unknown erasure test case")]
    UnknownCase,
    #[error("erasure test is already running")]
    ConcurrentCase,
    #[error("invalid erasure test setup")]
    Setup,
    #[error("allocation observation did not match its registration")]
    Observation,
    #[error("allocation residue did not match the expected cleanup")]
    Residue,
    #[error("element cleanup did not precede its destructor exactly once")]
    DropOrder,
    #[error("invalid map value unexpectedly decoded")]
    DecodeAccepted,
}

impl ErrorReason {
    fn code(&self) -> u32 {
        match self {
            Self::UnknownCase => 1,
            Self::ConcurrentCase => 2,
            Self::Setup => 3,
            Self::Observation => 4,
            Self::Residue => 5,
            Self::DropOrder => 6,
            Self::DecodeAccepted => 7,
        }
    }
}

/// Register an allocation whose bytes remain initialized through observation.
///
/// # Safety
/// This must be the allocation's base pointer and exact allocated size. Every
/// byte must be initialized, and subsequent moves/writes must not introduce
/// uninitialized padding. Ownership must keep it live until allocator release.
unsafe fn watch(pointer: *const u8, length: usize) -> Result<(), ErrorReason> {
    if pointer.is_null() || length == 0 {
        return Err(ErrorReason::Setup);
    }
    let index = WATCH_COUNT
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            count.checked_add(1).filter(|next| *next <= MAX_WATCHES)
        })
        .map_err(|_| ErrorReason::Setup)?;
    let slot = WATCHES.get(index).ok_or(ErrorReason::Setup)?;
    slot.length.store(length, Ordering::SeqCst);
    slot.pointer.store(pointer.cast_mut(), Ordering::SeqCst);
    Ok(())
}

// NonZeroU32 has no padding. It must remain valid until its destructor finishes;
// only the vacated allocation is allowed to become all-zero storage afterward.
#[repr(transparent)]
struct Slot(NonZeroU32);

impl Zeroize for Slot {
    fn zeroize(&mut self) {
        self.0 = NonZeroU32::MIN;
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if CHECK_DROPS.load(Ordering::SeqCst) {
            if self.0 != NonZeroU32::MIN {
                BAD_DROP.store(true, Ordering::SeqCst);
            }
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
        // SAFETY: write a valid, fully initialized representation before Drop
        // ends. The missing-post-drop-wipe control therefore remains observable
        // without reading a niche-invalid value or uninitialized storage.
        unsafe { std::ptr::write_volatile(&mut self.0, NonZeroU32::MAX) };
    }
}

fn watched_slots(length: usize) -> Result<Vec<Slot>, ErrorReason> {
    let mut slots = Vec::with_capacity(SLOT_CAPACITY);
    let capacity = slots.capacity();
    if length > capacity {
        return Err(ErrorReason::Setup);
    }
    slots.resize_with(capacity, || Slot(NonZeroU32::MAX));
    // Truncation deliberately leaves initialized marker bytes in spare slots.
    // This preparation precedes the cleanup-order assertions.
    slots.truncate(length);
    let size = capacity
        .checked_mul(std::mem::size_of::<Slot>())
        .ok_or(ErrorReason::Setup)?;
    // SAFETY: the complete allocation was initialized with padding-free Slots.
    // Every removed slot's destructor writes its entire valid representation.
    unsafe { watch(slots.as_ptr().cast(), size)? };
    CHECK_DROPS.store(true, Ordering::SeqCst);
    Ok(slots)
}

fn watched_text() -> Result<String, ErrorReason> {
    let mut bytes = vec![b'Q'; TEXT_CAPACITY];
    bytes.resize(bytes.capacity(), b'Q');
    let mut text = String::from_utf8(bytes).map_err(|_| ErrorReason::Setup)?;
    text.truncate(TEXT_LENGTH);
    // SAFETY: the live UTF-8 bytes and spare capacity were all initialized above.
    // This String is moved into an owner without modifying or growing its buffer.
    unsafe { watch(text.as_ptr(), text.capacity())? };
    Ok(text)
}

struct Expected {
    releases: usize,
    dirty: usize,
    drops: usize,
}

enum Case {
    VecWithoutFinalWipe,
    OrdinaryString,
    VecCleanup,
    EmptyVecCleanup,
    ListGrowth,
    ListDrop,
    PartialIterator,
    SecretString,
    NestedWkt,
    WktReplacement,
    MapDecodeError,
}

impl TryFrom<u32> for Case {
    type Error = ErrorReason;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        // Fixed test ABI shared with the JavaScript scenario table.
        match code {
            0 => Ok(Self::VecWithoutFinalWipe),
            1 => Ok(Self::OrdinaryString),
            2 => Ok(Self::VecCleanup),
            3 => Ok(Self::EmptyVecCleanup),
            4 => Ok(Self::ListGrowth),
            5 => Ok(Self::ListDrop),
            6 => Ok(Self::PartialIterator),
            7 => Ok(Self::SecretString),
            8 => Ok(Self::NestedWkt),
            9 => Ok(Self::WktReplacement),
            10 => Ok(Self::MapDecodeError),
            _ => Err(ErrorReason::UnknownCase),
        }
    }
}

fn execute(case: Case) -> Result<Expected, ErrorReason> {
    let expected = match case {
        // Negative control reproduces the missing final Vec allocation wipe.
        Case::VecWithoutFinalWipe => {
            let mut slots = watched_slots(1)?;
            // Model the missing final allocation pass independently of which
            // zeroize version implements Vec::zeroize in this consumer.
            for slot in &mut slots {
                slot.zeroize();
            }
            slots.clear();
            drop(slots);
            Expected {
                releases: 1,
                dirty: 1,
                drops: 1,
            }
        }
        // Negative control confirms the observer detects ordinary String residue.
        Case::OrdinaryString => {
            drop(watched_text()?);
            Expected {
                releases: 1,
                dirty: 1,
                drops: 0,
            }
        }
        Case::VecCleanup | Case::EmptyVecCleanup => {
            let length = usize::from(matches!(case, Case::VecCleanup));
            let mut slots = watched_slots(length)?;
            let pointer = slots.as_ptr();
            let capacity = slots.capacity();
            zeroize_vec(&mut slots);
            if !slots.is_empty() || slots.as_ptr() != pointer || slots.capacity() != capacity {
                return Err(ErrorReason::Observation);
            }
            drop(slots);
            Expected {
                releases: 1,
                dirty: 0,
                drops: length,
            }
        }
        Case::ListGrowth => {
            let slots = watched_slots(SLOT_CAPACITY)?;
            let drops = slots.len().checked_add(1).ok_or(ErrorReason::Setup)?;
            if slots.len() != slots.capacity() {
                return Err(ErrorReason::Setup);
            }
            let mut list = List::from(slots);
            list.push(Slot(NonZeroU32::MAX));
            // Growth must have released the old buffer before the new one drops.
            if RELEASES.load(Ordering::SeqCst) != 1 {
                return Err(ErrorReason::Observation);
            }
            drop(list);
            Expected {
                releases: 1,
                dirty: 0,
                drops,
            }
        }
        Case::ListDrop => {
            drop(List::from(watched_slots(1)?));
            Expected {
                releases: 1,
                dirty: 0,
                drops: 1,
            }
        }
        Case::PartialIterator => {
            let mut iterator = List::from(watched_slots(SLOT_CAPACITY)?).into_iter();
            let mut front = iterator.next().ok_or(ErrorReason::Setup)?;
            let mut back = iterator.next_back().ok_or(ErrorReason::Setup)?;
            // Transferred elements are explicitly the caller's responsibility.
            front.zeroize();
            back.zeroize();
            drop(front);
            drop(back);
            drop(iterator);
            Expected {
                releases: 1,
                dirty: 0,
                drops: SLOT_CAPACITY,
            }
        }
        Case::SecretString => {
            drop(SecretString::from(watched_text()?));
            Expected {
                releases: 1,
                dirty: 0,
                drops: 0,
            }
        }
        Case::NestedWkt => {
            let mut inner = Struct::default();
            inner.insert(
                SecretString::from(watched_text()?),
                SecretString::from(watched_text()?),
            );
            let mut list = ListValue::default();
            list.push(inner);
            let mut outer = Struct::default();
            outer.insert(SecretString::from(watched_text()?), list);
            drop(outer);
            Expected {
                releases: 3,
                dirty: 0,
                drops: 0,
            }
        }
        Case::WktReplacement => {
            let mut value = Struct::default();
            value.insert(
                SecretString::from(watched_text()?),
                SecretString::from(watched_text()?),
            );
            value.insert(
                SecretString::from(watched_text()?),
                SecretString::from(watched_text()?),
            );
            // The replaced value and duplicate incoming key must already be gone.
            if RELEASES.load(Ordering::SeqCst) != 2 {
                return Err(ErrorReason::Observation);
            }
            drop(value);
            Expected {
                releases: 4,
                dirty: 0,
                drops: 0,
            }
        }
        Case::MapDecodeError => {
            let source = serde::de::value::MapDeserializer::<_, serde::de::value::Error>::new(
                std::iter::once((watched_text()?, true)),
            );
            if Map::<SecretString, u32>::deserialize(source).is_ok() {
                return Err(ErrorReason::DecodeAccepted);
            }
            // A decoded key must be erased when decoding its value fails.
            Expected {
                releases: 1,
                dirty: 0,
                drops: 0,
            }
        }
    };
    Ok(expected)
}

struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        CHECK_DROPS.store(false, Ordering::SeqCst);
        RUNNING.store(false, Ordering::SeqCst);
    }
}

fn run(case: u32) -> Result<(), ErrorReason> {
    RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| ErrorReason::ConcurrentCase)?;
    let _running = Running;
    if WATCHES
        .iter()
        .any(|watch| !watch.pointer.load(Ordering::SeqCst).is_null())
    {
        return Err(ErrorReason::Observation);
    }
    WATCH_COUNT.store(0, Ordering::SeqCst);
    RELEASES.store(0, Ordering::SeqCst);
    DIRTY_RELEASES.store(0, Ordering::SeqCst);
    BAD_OBSERVER.store(false, Ordering::SeqCst);
    CHECK_DROPS.store(false, Ordering::SeqCst);
    BAD_DROP.store(false, Ordering::SeqCst);
    DROPS.store(0, Ordering::SeqCst);
    let expected = execute(Case::try_from(case)?)?;
    if BAD_OBSERVER.load(Ordering::SeqCst)
        || WATCH_COUNT.load(Ordering::SeqCst) != expected.releases
        || RELEASES.load(Ordering::SeqCst) != expected.releases
        || WATCHES
            .iter()
            .any(|watch| !watch.pointer.load(Ordering::SeqCst).is_null())
    {
        return Err(ErrorReason::Observation);
    }
    if DIRTY_RELEASES.load(Ordering::SeqCst) != expected.dirty {
        return Err(ErrorReason::Residue);
    }
    if BAD_DROP.load(Ordering::SeqCst) || DROPS.load(Ordering::SeqCst) != expected.drops {
        return Err(ErrorReason::DropOrder);
    }
    Ok(())
}

/// Fixed test ABI version; no application-facing API is exposed by this crate.
#[no_mangle]
pub extern "C" fn erasure_abi_version() -> u32 {
    1
}

/// Number of cases, including two controls that must observe nonzero residue.
#[no_mangle]
pub extern "C" fn erasure_case_count() -> u32 {
    CASE_COUNT
}

/// Run one isolated fixture, returning a fixed status code without input data.
#[no_mangle]
pub extern "C" fn erasure_run_case(case: u32) -> u32 {
    match run(case) {
        Ok(()) => 0,
        Err(error) => error.code(),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn cleanup_observations_and_negative_controls() {
        for case in 0..super::CASE_COUNT {
            assert_eq!(super::erasure_run_case(case), 0, "case {case}");
        }
        assert_eq!(super::erasure_run_case(u32::MAX), 1);
    }
}
