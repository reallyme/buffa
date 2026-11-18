// Modified by ReallyMe Technologies Limited.
//! Compare borrowed export with the previous owned-conversion path, including
//! bytes already written on failure. Allocation counts exclude fixture setup.

#![cfg(all(feature = "sensitive", feature = "json"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{self, Write};

use buffa::{DecodeOptions, LazyMessageView, Message, MessageView, ViewEncode};
use buffa_types::sensitive::{
    Duration, DurationLazyView, DurationView, ListValue, ListValueLazyView, ListValueView,
    SecretString, Struct, StructLazyView, StructView, Timestamp, TimestampLazyView, TimestampView,
    Value, ValueLazyView, ValueView,
};
use serde::Serialize;

const SINK_CAPACITY: usize = 262_144;
const VIEW_ERROR: &str = "invalid guarded WKT JSON representation (View)";

#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    allocations: usize,
    bytes: usize,
    live_bytes: usize,
    peak_bytes: usize,
    accounting_error: bool,
}

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts { allocations: 0, bytes: 0, live_bytes: 0, peak_bytes: 0, accounting_error: false }) };
}

fn record(size: usize) {
    // Constant-initialized, destructor-free TLS does not allocate. Other test
    // threads are deliberately excluded from an allocation measurement.
    if matches!(COUNTING.try_with(Cell::get), Ok(true)) {
        let _ = COUNTS.try_with(|slot| {
            let current = slot.get();
            if let (Some(allocations), Some(bytes), Some(live_bytes)) = (
                current.allocations.checked_add(1),
                current.bytes.checked_add(size),
                current.live_bytes.checked_add(size),
            ) {
                slot.set(Counts {
                    allocations,
                    bytes,
                    live_bytes,
                    peak_bytes: current.peak_bytes.max(live_bytes),
                    accounting_error: current.accounting_error,
                });
            } else {
                let mut failed = current;
                failed.accounting_error = true;
                slot.set(failed);
            }
        });
    }
}

fn release(size: usize) {
    // Measured fixtures borrow their pre-existing inputs and reserve sink
    // storage beforehand. Every temporary released during measurement was
    // allocated in that same scope, so byte accounting excludes caller storage.
    if matches!(COUNTING.try_with(Cell::get), Ok(true)) {
        let _ = COUNTS.try_with(|slot| {
            let mut current = slot.get();
            if let Some(live_bytes) = current.live_bytes.checked_sub(size) {
                current.live_bytes = live_bytes;
            } else {
                current.accounting_error = true;
            }
            slot.set(current);
        });
    }
}

struct CountingAllocator;

// SAFETY: every operation forwards the original System pointer/layout pair.
// Recording only updates nonallocating thread-local counters; no allocation's
// contents, private layout, or released bytes are inspected.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplied the allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplied the allocation layout.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        release(layout.size());
        // SAFETY: pointer and layout come unchanged from GlobalAlloc's caller.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: all allocation arguments come from GlobalAlloc's caller.
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            release(layout.size());
            record(size);
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measure<T>(operation: impl FnOnce() -> T) -> (T, Counts) {
    assert!(!COUNTING.with(Cell::get));
    COUNTS.with(|slot| slot.set(Counts::default()));
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = COUNTING.try_with(|enabled| enabled.set(false));
        }
    }
    COUNTING.with(|enabled| enabled.set(true));
    let stop = Stop;
    let result = operation();
    drop(stop);
    let counts = COUNTS.with(Cell::get);
    assert!(!counts.accounting_error, "invalid allocation accounting");
    (result, counts)
}

struct Sink {
    bytes: Vec<u8>,
    limit: usize,
    writes: usize,
}

impl Sink {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(SINK_CAPACITY),
            limit: limit.min(SINK_CAPACITY),
            writes: 0,
        }
    }
}

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes = self.writes.checked_add(1).ok_or(io::ErrorKind::Other)?;
        if bytes.is_empty() {
            return Ok(0);
        }
        let remaining = self
            .limit
            .checked_sub(self.bytes.len())
            .ok_or(io::ErrorKind::BrokenPipe)?;
        if remaining == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let count = remaining.min(bytes.len());
        let input = bytes.get(..count).ok_or(io::ErrorKind::InvalidInput)?;
        // The fixed limit never exceeds the capacity reserved before counting.
        self.bytes.extend_from_slice(input);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Failure {
    diagnostic: String,
    categories: [bool; 4],
    line: usize,
    column: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    bytes: Vec<u8>,
    failure: Option<Failure>,
    writes: usize,
}

fn snapshot(
    limit: usize,
    operation: impl FnOnce(&mut Sink) -> Result<(), serde_json::Error>,
) -> Snapshot {
    let mut sink = Sink::new(limit);
    let failure = operation(&mut sink).err().map(|error| Failure {
        diagnostic: error.to_string(),
        categories: [
            error.is_io(),
            error.is_data(),
            error.is_syntax(),
            error.is_eof(),
        ],
        line: error.line(),
        column: error.column(),
    });
    Snapshot {
        bytes: sink.bytes,
        failure,
        writes: sink.writes,
    }
}

fn previous_eager<'wire, V: MessageView<'wire>>(
    view: &V,
    sink: &mut Sink,
) -> Result<(), serde_json::Error>
where
    V::Owned: Serialize,
{
    let owned = view
        .to_owned_message()
        .map_err(|_| <serde_json::Error as serde::ser::Error>::custom(VIEW_ERROR))?;
    serde_json::to_writer(sink, &owned)
}

fn previous_lazy<'wire, V: LazyMessageView<'wire>>(
    view: &V,
    sink: &mut Sink,
) -> Result<(), serde_json::Error>
where
    V::Owned: Serialize,
{
    let owned = view
        .to_owned_message()
        .map_err(|_| <serde_json::Error as serde::ser::Error>::custom(VIEW_ERROR))?;
    serde_json::to_writer(sink, &owned)
}

fn compare_eager<'wire, V: MessageView<'wire> + Serialize>(view: &V, limit: usize)
where
    V::Owned: Serialize,
{
    assert_eq!(
        snapshot(limit, |sink| serde_json::to_writer(sink, view)),
        snapshot(limit, |sink| previous_eager(view, sink)),
    );
}

fn compare_lazy<'wire, V: LazyMessageView<'wire> + Serialize>(view: &V, limit: usize)
where
    V::Owned: Serialize,
{
    assert_eq!(
        snapshot(limit, |sink| serde_json::to_writer(sink, view)),
        snapshot(limit, |sink| previous_lazy(view, sink)),
    );
}

fn samples() -> Vec<Value> {
    let mut values = vec![
        Value::null(),
        Value::number(0.0),
        Value::number(-0.0),
        Value::number(f64::MIN_POSITIVE),
        Value::number(f64::MAX),
        Value::from(true),
        Value::from(false),
        Value::from(SecretString::from("\"\\\n\r\t\0é😀")),
        Value::from(Struct::default()),
        Value::from(ListValue::default()),
        Value::default(),
        Value::number(f64::NAN),
        Value::number(f64::INFINITY),
        Value::number(f64::NEG_INFINITY),
        Value {
            kind: Some(
                buffa_types::sensitive::__buffa::oneof::value::Kind::NullValue(
                    buffa::EnumValue::Unknown(99),
                ),
            ),
        },
    ];
    for input in [
        r#"{"z":[null,true,"private",{"😀":"é","":-0.0}],"a":1.25}"#,
        r#"[[["x"]],{"nested":[]} ]"#,
    ] {
        values.push(serde_json::from_str(input).unwrap());
    }
    values
}

#[test]
fn dynamic_variants_nested_trees_and_writer_failures_match_previous_export() {
    for value in samples() {
        let wire = value.encode_to_vec();
        let eager = ValueView::decode_view(&wire).unwrap();
        let lazy = ValueLazyView::decode_lazy(&wire).unwrap();
        let before = eager.encode_to_vec();
        for limit in [SINK_CAPACITY, 0, 1, 2, 5, 11, 29, 63] {
            compare_eager(&eager, limit);
            compare_lazy(&lazy, limit);
        }
        assert_eq!(eager.encode_to_vec(), before);
        drop(eager);
        drop(lazy);
        let owned = buffa::OwnedView::<ValueView<'static>>::decode(wire.into()).unwrap();
        let reference = owned.to_owned_message();
        assert_eq!(
            snapshot(SINK_CAPACITY, |sink| serde_json::to_writer(sink, &owned)),
            snapshot(SINK_CAPACITY, |sink| serde_json::to_writer(
                sink, &reference
            )),
        );
    }
}

#[test]
fn sorted_map_keys_last_duplicate_and_discarded_invalid_values_are_preserved() {
    use buffa_types::sensitive::__buffa::view::oneof::value::Kind;
    let mut fields = buffa::sensitive::MapView::default();
    for (key, kind) in [
        ("z", Some(Kind::NumberValue(f64::NAN))),
        ("a", None),
        ("é", Some(Kind::StringValue("escaped\"\n"))),
        ("", Some(Kind::BoolValue(true))),
        ("a", Some(Kind::NumberValue(2.5))),
        ("😀", Some(Kind::StringValue("last"))),
        ("z", Some(Kind::BoolValue(false))),
    ] {
        fields.push(key, ValueView { kind });
    }
    let eager = StructView { fields };
    let lazy = StructLazyView {
        fields: eager.fields.clone(),
    };
    let expected = "{\"\":true,\"a\":2.5,\"z\":false,\"é\":\"escaped\\\"\\n\",\"😀\":\"last\"}";
    assert_eq!(serde_json::to_string(&eager).unwrap(), expected);
    assert_eq!(serde_json::to_string(&lazy).unwrap(), expected);
    let before = eager.encode_to_vec();
    for limit in 0..=expected.len().checked_add(1).unwrap() {
        compare_eager(&eager, limit);
        compare_lazy(&lazy, limit);
    }
    assert_eq!(eager.encode_to_vec(), before);
    assert_eq!(eager.fields.len(), 7);

    for entries in [vec![], vec![("one", ValueView::default())]] {
        let eager = StructView {
            fields: entries.into(),
        };
        let lazy = StructLazyView {
            fields: eager.fields.clone(),
        };
        compare_eager(&eager, SINK_CAPACITY);
        compare_lazy(&lazy, SINK_CAPACITY);
    }
}

#[test]
fn map_permutations_preserve_duplicate_errors_and_escaped_key_order() {
    use buffa_types::sensitive::__buffa::view::oneof::value::Kind;
    let keys = ["", "a", "a\n", "é", "e\u{301}", "😀", "z", "\0"];
    for rotation in 0_usize..keys.len() {
        let mut fields = buffa::sensitive::MapView::default();
        for index in 0_usize..40 {
            let offset = index.checked_add(rotation).unwrap();
            let key = keys.get(offset % keys.len()).unwrap();
            let kind = match offset % 5 {
                0 => None,
                1 => Some(Kind::NumberValue(f64::NAN)),
                2 => Some(Kind::StringValue("escaped\"\\\n😀")),
                3 => Some(Kind::BoolValue(true)),
                _ => Some(Kind::NumberValue(f64::from(u32::try_from(index).unwrap()))),
            };
            fields.push(*key, ValueView { kind });
        }
        let eager = StructView { fields };
        let lazy = StructLazyView {
            fields: eager.fields.clone(),
        };
        for limit in [0, 1, 5, 12, 31, SINK_CAPACITY] {
            compare_eager(&eager, limit);
            compare_lazy(&lazy, limit);
        }
    }
}

fn raw_list(children: &[Vec<u8>]) -> Vec<u8> {
    let mut wire = Vec::new();
    for child in children {
        assert!(
            child.len() < 128,
            "fixture uses a single-byte length prefix"
        );
        wire.push(0x0a);
        wire.push(u8::try_from(child.len()).unwrap());
        wire.extend_from_slice(child);
    }
    wire
}

#[test]
fn list_views_keep_partial_output_and_fixed_json_diagnostics() {
    for final_value in samples() {
        let mut value = ListValue::default();
        value.push(Value::number(1.25));
        value.push(final_value);
        let wire = value.encode_to_vec();
        let eager = ListValueView::decode_view(&wire).unwrap();
        let lazy = ListValueLazyView::decode_lazy(&wire).unwrap();
        for limit in [SINK_CAPACITY, 0, 1, 4, 6, 13] {
            compare_eager(&eager, limit);
            compare_lazy(&lazy, limit);
        }
        assert_eq!(lazy.encode_to_vec(), wire);
    }
    let eager = ListValueView::decode_view(&[]).unwrap();
    let lazy = ListValueLazyView::decode_lazy(&[]).unwrap();
    compare_eager(&eager, SINK_CAPACITY);
    compare_lazy(&lazy, SINK_CAPACITY);
}

#[test]
fn deferred_wire_failures_precede_json_errors_and_all_writer_calls() {
    for first in [
        Value::number(1.25),
        Value::number(f64::NAN),
        Value::default(),
    ] {
        for malformed in [
            vec![0],
            vec![0x08, 0x80],
            vec![0x1a, 2, b'x'],
            vec![0x1a, 1, 0xff],
        ] {
            let wire = raw_list(&[first.encode_to_vec(), malformed]);
            let lazy = ListValueLazyView::decode_lazy(&wire).unwrap();
            assert!(lazy.to_owned_message().is_err());
            for limit in [0, SINK_CAPACITY] {
                compare_lazy(&lazy, limit);
                let actual = snapshot(limit, |sink| serde_json::to_writer(sink, &lazy));
                assert_eq!(actual.writes, 0);
                assert!(actual.bytes.is_empty());
                assert_eq!(actual.failure.unwrap().diagnostic, VIEW_ERROR);
            }
        }
    }
}

#[test]
fn recorded_unknown_recursion_and_memory_limits_keep_their_error_priority() {
    let unknown = raw_list(&[vec![0x08, 0, 0xa0, 6, 1]]);
    let lazy = DecodeOptions::new()
        .reject_unknown_fields()
        .decode_lazy_view::<ListValueLazyView<'_>>(&unknown)
        .unwrap();
    assert!(lazy.to_owned_message().is_err());
    compare_lazy(&lazy, 0);

    let nested = raw_list(&[vec![0x2a, 0]]);
    let lazy = DecodeOptions::new()
        .with_recursion_limit(1)
        .decode_lazy_view::<ListValueLazyView<'_>>(&nested)
        .unwrap();
    assert!(lazy.to_owned_message().is_err());
    compare_lazy(&lazy, 0);

    let mut object = Struct::default();
    object.insert("a", true);
    object.insert("b", false);
    let wire = raw_list(&[Value::from(object).encode_to_vec()]);
    // Charge the outer deferred reference and exactly one map entry. Using a
    // fixed byte budget would accept both entries on smaller-pointer targets.
    let entry_memory = core::mem::size_of::<&str>()
        .checked_add(core::mem::size_of::<ValueView<'_>>())
        .unwrap();
    let memory_limit = core::mem::size_of::<&[u8]>()
        .checked_add(entry_memory)
        .unwrap();
    let lazy = DecodeOptions::new()
        .with_element_memory_limit(memory_limit)
        .decode_lazy_view::<ListValueLazyView<'_>>(&wire)
        .unwrap();
    assert!(lazy.to_owned_message().is_err());
    compare_lazy(&lazy, 0);
}

#[test]
fn temporal_views_retain_canonical_output_and_fixed_errors() {
    for value in [
        Timestamp::default(),
        Timestamp {
            seconds: -62_135_596_800,
            nanos: 1,
        },
        Timestamp {
            seconds: 253_402_300_799,
            nanos: 999_999_999,
        },
        Timestamp {
            seconds: i64::MAX,
            nanos: 0,
        },
        Timestamp {
            seconds: 0,
            nanos: -1,
        },
    ] {
        let wire = value.encode_to_vec();
        let eager = TimestampView::decode_view(&wire).unwrap();
        let lazy = TimestampLazyView::decode_lazy(&wire).unwrap();
        for limit in [SINK_CAPACITY, 0, 1, 8] {
            compare_eager(&eager, limit);
            compare_lazy(&lazy, limit);
        }
    }
    for value in [
        Duration::default(),
        Duration {
            seconds: -1,
            nanos: -1,
        },
        Duration {
            seconds: 315_576_000_000,
            nanos: 999_999_999,
        },
        Duration {
            seconds: 1,
            nanos: -1,
        },
        Duration {
            seconds: i64::MIN,
            nanos: 0,
        },
    ] {
        let wire = value.encode_to_vec();
        let eager = DurationView::decode_view(&wire).unwrap();
        let lazy = DurationLazyView::decode_lazy(&wire).unwrap();
        for limit in [SINK_CAPACITY, 0, 1, 8] {
            compare_eager(&eager, limit);
            compare_lazy(&lazy, limit);
        }
    }
}

#[test]
fn borrowed_map_export_avoids_owned_payload_copies_and_per_field_allocations() {
    let count: usize = if cfg!(miri) { 16 } else { 1_000 };
    for text_length in [8, 128] {
        let text = "Q".repeat(text_length);
        let mut object = Struct::default();
        for index in (0..count).rev() {
            object.insert(
                format!("field-{index:04}"),
                SecretString::from(text.as_str()),
            );
        }
        let wire = object.encode_to_vec();
        let eager = StructView::decode_view(&wire).unwrap();
        let lazy = StructLazyView::decode_lazy(&wire).unwrap();
        let mut owned_sink = Sink::new(SINK_CAPACITY);
        let mut eager_sink = Sink::new(SINK_CAPACITY);
        let mut lazy_sink = Sink::new(SINK_CAPACITY);
        let mut previous_sink = Sink::new(SINK_CAPACITY);
        let (result, owned) = measure(|| serde_json::to_writer(&mut owned_sink, &object));
        result.unwrap();
        let (result, previous) = measure(|| previous_eager(&eager, &mut previous_sink));
        result.unwrap();
        let (result, current) = measure(|| serde_json::to_writer(&mut eager_sink, &eager));
        result.unwrap();
        let (result, current_lazy) = measure(|| serde_json::to_writer(&mut lazy_sink, &lazy));
        result.unwrap();
        assert_eq!(owned.allocations, 0);
        assert!(previous.allocations > count);
        assert!(current.allocations <= 1, "{current:?}");
        assert!(current_lazy.allocations <= 1, "{current_lazy:?}");
        let metadata_bound = count
            .checked_mul(8 * core::mem::size_of::<usize>())
            .unwrap();
        assert!(current.bytes <= metadata_bound);
        assert!(current_lazy.bytes <= metadata_bound);
        assert!(current.bytes < previous.bytes);
        assert_eq!(eager_sink.bytes, owned_sink.bytes);
        assert_eq!(lazy_sink.bytes, owned_sink.bytes);
        assert_eq!(previous_sink.bytes, owned_sink.bytes);
    }
}

#[test]
fn borrowed_list_export_avoids_owned_payload_copies_and_per_element_allocations() {
    let count: usize = if cfg!(miri) { 16 } else { 1_000 };
    let mut list = ListValue::default();
    for _ in 0..count {
        list.push(SecretString::from("borrowed-payload"));
    }
    let wire = list.encode_to_vec();
    let eager = ListValueView::decode_view(&wire).unwrap();
    let lazy = ListValueLazyView::decode_lazy(&wire).unwrap();
    let mut eager_sink = Sink::new(SINK_CAPACITY);
    let mut lazy_sink = Sink::new(SINK_CAPACITY);
    let (result, current) = measure(|| serde_json::to_writer(&mut eager_sink, &eager));
    result.unwrap();
    let (result, current_lazy) = measure(|| serde_json::to_writer(&mut lazy_sink, &lazy));
    result.unwrap();
    assert_eq!(current.allocations, 0, "{current:?}");
    assert_eq!(current_lazy.allocations, 0, "{current_lazy:?}");
    assert_eq!(eager_sink.bytes, lazy_sink.bytes);
}

#[test]
fn lazy_lists_bound_temporary_views_even_with_many_duplicate_map_entries() {
    use buffa_types::sensitive::__buffa::view::oneof::value::Kind;
    let mut fields = buffa::sensitive::MapView::default();
    for index in 0_u32..6 {
        fields.push(
            "x",
            ValueView {
                kind: Some(Kind::NumberValue(f64::from(index))),
            },
        );
    }
    let child = ValueView {
        kind: Some(Kind::StructValue(Box::new(StructView { fields }))),
    };
    let child_wire = child.encode_to_vec();
    let mut peaks = Vec::new();
    for count in [1, 32] {
        let wire = raw_list(&vec![child_wire.clone(); count]);
        let lazy = ListValueLazyView::decode_lazy(&wire).unwrap();
        let mut sink = Sink::new(SINK_CAPACITY);
        let (result, current) = measure(|| serde_json::to_writer(&mut sink, &lazy));
        result.unwrap();
        assert_eq!(
            current.live_bytes, 0,
            "temporary child storage must be released"
        );
        peaks.push(current.peak_bytes);
        compare_lazy(&lazy, SINK_CAPACITY);
    }
    assert!(peaks[0] > 0);
    assert!(peaks[1] <= peaks[0].checked_mul(2).unwrap(), "{peaks:?}");
}
