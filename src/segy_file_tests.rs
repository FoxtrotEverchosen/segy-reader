//! Unit tests for `segy_file.rs`: opening files, the trace index, and trace/range access.
//!
//! Files come from `test_support::SegyBuilder` (hand-computed offsets, no production writer).
//! Only the pure-Rust methods are tested here (`open_segy`, `build_trace_index`,
//! `get_trace_data`, `get_trace_range_data`). The `#[pymethods]` wrappers need a Python
//! interpreter and belong in pytest.

use super::*;
use crate::test_support::{SegyBuilder, file_bytes};
use std::io::Write;
use tempfile::NamedTempFile;

const ORDERS: [ByteOrder; 3] = [ByteOrder::BigEndian, ByteOrder::LittleEndian, ByteOrder::SwappedWord];

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// Opens a file from disk. `PyErr` is not printed on failure because formatting it needs a
/// running interpreter.
fn open_path(path: &str) -> SegyFile {
    match SegyFile::open_segy(path) {
        Ok(f) => f,
        Err(_) => panic!("open_segy({path}) failed"),
    }
}

/// Writes a built file to disk and opens it. Keep both values alive: the file is mapped lazily
/// and the temp file is deleted when its handle is dropped.
fn open(b: &SegyBuilder) -> (SegyFile, NamedTempFile) {
    let tmp = b.write_tmp();
    let file = open_path(tmp.path().to_str().unwrap());
    (file, tmp)
}

/// Whether `open_segy` accepts these exact bytes.
fn opens_ok(bytes: &[u8]) -> bool {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(bytes).unwrap();
    tmp.flush().unwrap();
    SegyFile::open_segy(tmp.path().to_str().unwrap()).is_ok()
}

fn f32s(t: TraceData) -> Vec<f32> {
    match t {
        TraceData::F32(v) => v,
        _ => panic!("expected TraceData::F32"),
    }
}

/// Three traces of four IEEE f32 samples each.
fn three_traces(order: ByteOrder) -> SegyBuilder {
    SegyBuilder::new(order, 5)
        .samples(4)
        .trace_f32(&[1.0, 2.0, 3.0, 4.0])
        .trace_f32(&[5.0, 6.0, 7.0, 8.0])
        .trace_f32(&[9.0, 10.0, 11.0, 12.0])
}

/// Five traces; trace k (1-based) is `[k, k, k, k]`.
fn five_traces(order: ByteOrder) -> SegyBuilder {
    let mut b = SegyBuilder::new(order, 5).samples(4);
    for k in 1..=5_u8 {
        b = b.trace_f32(&[f32::from(k); 4]);
    }
    b
}

fn decode_one(code: i16, raw: &[u8]) -> Result<TraceData, SegyError> {
    let b = SegyBuilder::new(ByteOrder::BigEndian, code)
        .samples(2)
        .trace_bytes(raw.to_vec());
    let (f, _tmp) = open(&b);
    f.get_trace_data(1)
}

// ---------------------------------------------------------------------------------------------
// open_segy
// ---------------------------------------------------------------------------------------------

#[test]
fn open_rejects_files_too_short_for_text_and_binary_headers() {
    assert!(!opens_ok(&[]));
    assert!(!opens_ok(&vec![0_u8; 3599]));
}

#[test]
fn open_rejects_missing_file() {
    assert!(SegyFile::open_segy("/nonexistent/dir/missing.sgy").is_err());
}

#[test]
fn open_rejects_unrecognised_byte_order_indicator() {
    let mut bytes = three_traces(ByteOrder::BigEndian).build();
    bytes[3296..3300].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    assert!(!opens_ok(&bytes));
}

#[test]
fn open_rejects_revision_2() {
    let bytes = three_traces(ByteOrder::BigEndian).rev(2).build();
    assert!(!opens_ok(&bytes));
}

#[test]
fn open_rejects_undefined_data_format_code() {
    let bytes = SegyBuilder::new(ByteOrder::BigEndian, 13).build();
    assert!(!opens_ok(&bytes));
}

#[test]
fn open_accepts_a_file_with_headers_only() {
    let (f, _tmp) = open(&SegyBuilder::new(ByteOrder::BigEndian, 5));
    assert_eq!(f.trace_count, 0);
    assert!(f.trace_index.is_empty());
}

// ---------------------------------------------------------------------------------------------
// build_trace_index (exercised through open_segy)
// ---------------------------------------------------------------------------------------------

#[test]
fn index_fixed_length_traces() {
    for order in ORDERS {
        let b = three_traces(order);
        let (f, _tmp) = open(&b);
        assert_eq!(f.trace_count, 3, "{order:?}");
        assert_eq!(f.trace_index, b.trace_offsets(), "{order:?}");
    }
}

#[test]
fn index_offsets_literal() {
    // 3600 bytes of headers, then traces of 240 + 4 * 4 = 256 bytes each
    let (f, _tmp) = open(&three_traces(ByteOrder::BigEndian));
    assert_eq!(f.trace_index, vec![3600, 3856, 4112]);
}

#[test]
fn index_skips_extended_text_headers() {
    for order in ORDERS {
        let b = three_traces(order).ext_header("EXT 1").ext_header("EXT 2");
        let (f, _tmp) = open(&b);
        // 3600 + 2 * 3200 = 10000
        assert_eq!(f.trace_index, vec![10_000, 10_256, 10_512], "{order:?}");
    }
}

#[test]
fn index_uses_binary_header_count_when_trace_header_count_is_zero() {
    for order in ORDERS {
        let b = SegyBuilder::new(order, 5)
            .samples(4)
            .trace_f32_with_header_samples(0, &[1.0, 2.0, 3.0, 4.0])
            .trace_f32_with_header_samples(0, &[5.0, 6.0, 7.0, 8.0]);
        let (f, _tmp) = open(&b);
        assert_eq!(f.trace_index, vec![3600, 3856], "{order:?}");
        assert_eq!(f32s(f.get_trace_data(2).unwrap()), vec![5.0, 6.0, 7.0, 8.0]);
    }
}

/// The binary header says 4 samples, but the trace headers say 2, 5 and 3.
/// The trace header is the more reliable source, so the index must follow it.
#[test]
fn index_follows_trace_header_counts_for_variable_length_traces() {
    for order in ORDERS {
        let b = SegyBuilder::new(order, 5)
            .samples(4)
            .trace_f32_with_header_samples(2, &[1.0, 2.0])
            .trace_f32_with_header_samples(5, &[3.0, 4.0, 5.0, 6.0, 7.0])
            .trace_f32_with_header_samples(3, &[8.0, 9.0, 10.0]);
        let (f, _tmp) = open(&b);

        // 3600; +240+8 = 3848; +240+20 = 4108
        assert_eq!(f.trace_index, vec![3600, 3848, 4108], "{order:?}");
        assert_eq!(f.trace_count, 3);
        assert_eq!(f32s(f.get_trace_data(1).unwrap()), vec![1.0, 2.0]);
        assert_eq!(f32s(f.get_trace_data(2).unwrap()), vec![3.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!(f32s(f.get_trace_data(3).unwrap()), vec![8.0, 9.0, 10.0]);
    }
}

/// Trace header sample counts are unsigned 16-bit values, so a 40,000-sample trace has the high
/// bit set and must not be treated as negative.
#[test]
fn index_handles_trace_header_counts_above_i16_max() {
    let values = vec![1.5_f32; 40_000];
    let b = SegyBuilder::new(ByteOrder::BigEndian, 5)
        .samples(1000)
        .trace_f32_with_header_samples(40_000, &values)
        .trace_f32_with_header_samples(40_000, &values);
    let (f, _tmp) = open(&b);

    // second trace starts after 240 + 40,000 * 4 bytes
    assert_eq!(f.trace_index, vec![3600, 3600 + 240 + 160_000]);
    assert_eq!(f32s(f.get_trace_data(2).unwrap()).len(), 40_000);
}

#[test]
fn index_drops_a_truncated_final_trace() {
    // each trace is 256 bytes: a 240-byte header + 16 bytes of samples
    let cuts = [
        (1, "one byte short"),
        (10, "cut inside the samples"),
        (16, "samples missing, header complete"),
        (16 + 100, "cut inside the trace header"),
        (16 + 240, "last trace missing entirely"),
    ];
    for (cut, what) in cuts {
        let (f, _tmp) = open(&three_traces(ByteOrder::BigEndian).truncate_tail(cut));
        assert_eq!(f.trace_count, 2, "{what}");
        assert_eq!(f.trace_index, vec![3600, 3856], "{what}");
    }
}

// ---------------------------------------------------------------------------------------------
// get_trace_data
// ---------------------------------------------------------------------------------------------

#[test]
fn get_trace_returns_each_trace_in_every_byte_order() {
    for order in ORDERS {
        let (f, _tmp) = open(&three_traces(order));
        assert_eq!(f32s(f.get_trace_data(1).unwrap()), vec![1.0, 2.0, 3.0, 4.0], "{order:?}");
        assert_eq!(f32s(f.get_trace_data(2).unwrap()), vec![5.0, 6.0, 7.0, 8.0], "{order:?}");
        assert_eq!(f32s(f.get_trace_data(3).unwrap()), vec![9.0, 10.0, 11.0, 12.0], "{order:?}");
    }
}

#[test]
fn get_trace_numbers_are_one_based_and_bounded() {
    let (f, _tmp) = open(&three_traces(ByteOrder::BigEndian));

    assert!(matches!(f.get_trace_data(0), Err(SegyError::InvalidArgument(_))));
    assert!(matches!(
        f.get_trace_data(4),
        Err(SegyError::TraceOutOfRange { requested: 4, trace_count: 3 })
    ));
    assert!(matches!(
        f.get_trace_data(u32::MAX),
        Err(SegyError::TraceOutOfRange { trace_count: 3, .. })
    ));
}

#[test]
fn get_trace_on_a_file_without_traces_is_out_of_range() {
    let (f, _tmp) = open(&SegyBuilder::new(ByteOrder::BigEndian, 5));
    assert!(matches!(
        f.get_trace_data(1),
        Err(SegyError::TraceOutOfRange { requested: 1, trace_count: 0 })
    ));
}

/// Each data format code must reach the right decoder and produce the right element type.
/// (Big-endian only: byte order handling is covered in `decode_tests`.)
#[test]
fn every_format_code_reaches_the_right_decoder() {
    let ff8 = [0xFF; 8];
    let t = |code: i16, raw: &[u8]| decode_one(code, raw).unwrap();

    // 1: IBM f32   100.0 and -118.625
    assert!(matches!(
        t(1, &[0x42, 0x64, 0, 0, 0xC2, 0x76, 0xA0, 0x00]),
        TraceData::F32(v) if v == [100.0, -118.625]
    ));
    // 2: i32
    assert!(matches!(
        t(2, &[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x01, 0x02]),
        TraceData::I32(v) if v == [-1, 258]
    ));
    // 3: i16
    assert!(matches!(t(3, &[0xFF, 0xFE, 0x01, 0x02]), TraceData::I16(v) if v == [-2, 258]));
    // 5: IEEE f32
    assert!(matches!(
        t(5, &[0x3F, 0x80, 0, 0, 0xC0, 0x20, 0, 0]),
        TraceData::F32(v) if v == [1.0, -2.5]
    ));
    // 6: IEEE f64
    assert!(matches!(
        t(6, &[0x3F, 0xF0, 0, 0, 0, 0, 0, 0, 0xC0, 0x04, 0, 0, 0, 0, 0, 0]),
        TraceData::F64(v) if v == [1.0, -2.5]
    ));
    // 7: i24
    assert!(matches!(
        t(7, &[0xFF, 0xFF, 0xFF, 0x01, 0x02, 0x03]),
        TraceData::I24(v) if v == [-1, 66_051]
    ));
    // 8: i8
    assert!(matches!(t(8, &[0xFF, 0x7F]), TraceData::I8(v) if v == [-1, 127]));
    // 9: i64
    let raw: Vec<u8> = [&ff8[..], &[0, 0, 0, 0, 0, 0, 0x01, 0x02]].concat();
    assert!(matches!(t(9, &raw), TraceData::I64(v) if v == [-1, 258]));
    // 10: u32
    assert!(matches!(
        t(10, &[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x01, 0x02]),
        TraceData::U32(v) if v == [u32::MAX, 258]
    ));
    // 11: u16
    assert!(matches!(t(11, &[0xFF, 0xFF, 0x01, 0x02]), TraceData::U16(v) if v == [65_535, 258]));
    // 12: u64
    let raw: Vec<u8> = [&ff8[..], &[0, 0, 0, 0, 0, 0, 0x01, 0x02]].concat();
    assert!(matches!(t(12, &raw), TraceData::U64(v) if v == [u64::MAX, 258]));
    // 15: u24
    assert!(matches!(
        t(15, &[0xFF, 0xFF, 0xFF, 0x01, 0x02, 0x03]),
        TraceData::U24(v) if v == [16_777_215, 66_051]
    ));
    // 16: u8
    assert!(matches!(t(16, &[0xFF, 0x01]), TraceData::U8(v) if v == [255, 1]));
}

#[test]
fn fixed_point_with_gain_is_parsed_but_cannot_be_decoded() {
    // code 4 passes header parsing (4 bytes per sample) but decoding it is unsupported
    let raw = [0_u8; 8];
    assert!(matches!(decode_one(4, &raw), Err(SegyError::UnsupportedDataFormat)));
}

// ---------------------------------------------------------------------------------------------
// get_trace_range_data
// ---------------------------------------------------------------------------------------------

fn range(f: &SegyFile, start: u32, end: u32) -> Vec<Vec<f32>> {
    f.get_trace_range_data(start, end)
        .unwrap()
        .into_iter()
        .map(f32s)
        .collect()
}

/// Trace numbers are 1-based and `end` is inclusive: (2, 4) returns traces 2, 3 and 4.
#[test]
fn range_is_one_based_with_inclusive_end() {
    for order in ORDERS {
        let (f, _tmp) = open(&five_traces(order));
        assert_eq!(
            range(&f, 2, 4),
            vec![vec![2.0; 4], vec![3.0; 4], vec![4.0; 4]],
            "{order:?}"
        );
        assert_eq!(range(&f, 1, 5).len(), 5, "{order:?}");
        assert_eq!(range(&f, 4, 5), vec![vec![4.0; 4], vec![5.0; 4]], "{order:?}");
    }
}

#[test]
fn range_rejects_out_of_bounds_requests() {
    let (f, _tmp) = open(&five_traces(ByteOrder::BigEndian));

    assert!(matches!(
        f.get_trace_range_data(0, 3),
        Err(SegyError::InvalidTraceRange { start: 0, end: 3, trace_count: 5 })
    ));
    assert!(matches!(
        f.get_trace_range_data(2, 6),
        Err(SegyError::InvalidTraceRange { start: 2, end: 6, trace_count: 5 })
    ));
    assert!(matches!(
        f.get_trace_range_data(6, 9),
        Err(SegyError::InvalidTraceRange { .. })
    ));
}

#[test]
fn range_rejects_a_start_after_the_end() {
    let (f, _tmp) = open(&five_traces(ByteOrder::BigEndian));
    assert!(matches!(f.get_trace_range_data(4, 2), Err(SegyError::InvalidArgument(_))));
    assert!(matches!(f.get_trace_range_data(0, 0), Err(SegyError::InvalidArgument(_))));
}

#[test]
fn range_of_a_single_trace_is_rejected_for_now() {
    let (f, _tmp) = open(&five_traces(ByteOrder::BigEndian));
    assert!(matches!(f.get_trace_range_data(2, 2), Err(SegyError::InvalidArgument(_))));
}

#[test]
fn range_keeps_each_trace_its_own_length_for_variable_length_files() {
    let b = SegyBuilder::new(ByteOrder::BigEndian, 5)
        .samples(4)
        .trace_f32_with_header_samples(2, &[1.0, 2.0])
        .trace_f32_with_header_samples(5, &[3.0, 4.0, 5.0, 6.0, 7.0])
        .trace_f32_with_header_samples(3, &[8.0, 9.0, 10.0]);
    let (f, _tmp) = open(&b);

    // the Rust layer returns ragged data as-is; turning it into a 2-D array is the Python
    // wrapper's job (and where a ragged range has to fail cleanly)
    let lens: Vec<usize> = range(&f, 1, 3).iter().map(Vec::len).collect();
    assert_eq!(lens, vec![2, 5, 3]);
}

