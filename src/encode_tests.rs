//! Unit tests for `encode.rs` (`save_segy`).
//!
//! Files are written to disk and read back with `std::fs`, and the bytes are
//! compared against values worked out by hand from the SEG-Y spec. The writer is never checked
//! only against the production reader, so an offset mistake shared by both cannot hide.
//! The one exception is `saved_binary_header_is_accepted_by_the_parser`, which is a deliberate
//! cross-check between the two.
//!
//! `save_segy` writes `raw_traces` verbatim, so the tests pass samples already encoded in the
//! file's byte order. A full `save_segy` -> `SegyFile` round trip needs private items from
//! `segy_file.rs` and is done in pytest.

use super::*;
use crate::header::parse_binary_header;
use crate::test_support::ascii_to_ebcdic;

const ORDERS: [ByteOrder; 3] = [ByteOrder::BigEndian, ByteOrder::LittleEndian, ByteOrder::SwappedWord];

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// The byte-order indicator value for a file order, as the Python side passes it.
fn indicator(order: ByteOrder) -> u32 {
    match order {
        ByteOrder::BigEndian => 0x0102_0304,
        ByteOrder::LittleEndian => 0x0403_0201,
        ByteOrder::SwappedWord => 0x0201_0403,
    }
}

/// A valid config: IEEE f32, 4 samples, 2000 us, Rev 1.0 (0x0100), fixed-length flag set.
fn config(order: ByteOrder) -> BinaryHeaderConfig {
    BinaryHeaderConfig {
        sample_interval: 2000, // 0x07D0
        samples_per_trace: 4,
        data_format: 5,
        revision_number: 0x0100,
        fixed_length: 1,
        byte_order: indicator(order),
        bytes_per_sample: 4,
        ensemble_fold: None,
        trace_sorting_code: None,
        measurement_system: None,
    }
}

/// 16-bit value as stored in a file of the given order. Swapped-word equals little-endian
/// for 2-byte values.
fn u16_bytes(v: u16, order: ByteOrder) -> [u8; 2] {
    let [hi, lo] = v.to_be_bytes();
    match order {
        ByteOrder::BigEndian => [hi, lo],
        ByteOrder::LittleEndian | ByteOrder::SwappedWord => [lo, hi],
    }
}

/// Runs `save_segy` into a temp file and returns the file's bytes. `PyErr` is not printed on
/// failure because formatting it needs a running interpreter.
fn save(cfg: BinaryHeaderConfig, text: &str, raw: &[u8], ascii: bool, n_traces: usize) -> Vec<u8> {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    match save_segy(path, text, cfg, raw, ascii, n_traces) {
        Ok(()) => {}
        Err(_) => panic!("save_segy failed"),
    }
    std::fs::read(path).unwrap()
}

fn saves_ok(cfg: BinaryHeaderConfig, text: &str, raw: &[u8], ascii: bool, n_traces: usize) -> bool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.sgy");
    save_segy(path.to_str().unwrap(), text, cfg, raw, ascii, n_traces).is_ok()
}

/// "0123456789012..." so that every position holds a recognisable character and an off-by-one
/// shows up as a wrong digit.
fn pattern(len: usize) -> String {
    (0..len)
        .map(|i| char::from(b'0' + u8::try_from(i % 10).unwrap()))
        .collect()
}

/// The text blocks a correct writer produces, worked out independently of the encoder: the main
/// header carries at most 3120 bytes (the last 80-byte row stays padding), each extended header
/// the next 3200, and every block is padded to 3200 bytes.
fn expected_blocks(text: &str, ebcdic: bool) -> Vec<Vec<u8>> {
    let pad = if ebcdic { 0x40 } else { 0x20 };
    let convert = |b: u8| if ebcdic { ascii_to_ebcdic(b) } else { b };
    let block = |chunk: &[u8]| {
        let mut b = vec![pad; 3200];
        for (slot, &c) in b.iter_mut().zip(chunk) {
            *slot = convert(c);
        }
        b
    };

    let bytes = text.as_bytes();
    let (main, rest) = bytes.split_at(bytes.len().min(3120));
    let mut blocks = vec![block(main)];
    blocks.extend(rest.chunks(3200).map(block));
    blocks
}

// ---------------------------------------------------------------------------------------------
// Binary header
// ---------------------------------------------------------------------------------------------

/// Every byte of the 400-byte binary header, worked out by hand for a big-endian file.
#[test]
fn binary_header_golden_bytes_big_endian() {
    let bytes = save(config(ByteOrder::BigEndian), "", &[], true, 0);

    let mut expected = [0_u8; 400];
    expected[16..18].copy_from_slice(&[0x07, 0xD0]); // 3217-3218 sample interval = 2000
    expected[20..22].copy_from_slice(&[0x00, 0x04]); // 3221-3222 samples per trace = 4
    expected[24..26].copy_from_slice(&[0x00, 0x05]); // 3225-3226 data format = 5
    expected[96..100].copy_from_slice(&[0x01, 0x02, 0x03, 0x04]); // 3297-3300 byte order
    expected[300..302].copy_from_slice(&[0x01, 0x00]); // 3501-3502 revision 1.0
    expected[302..304].copy_from_slice(&[0x00, 0x01]); // 3503-3504 fixed length flag
    expected[304..306].copy_from_slice(&[0x00, 0x00]); // 3505-3506 extended headers

    assert_eq!(bytes.len(), 3600);
    assert_eq!(&bytes[3200..3600], &expected[..]);
}

/// 16-bit fields follow the file's byte order; the indicator itself is always written as the
/// big-endian bytes of the constant, whatever the file's order. (Revision bytes are checked
/// separately in `revision_is_major_byte_then_minor_byte`.)
#[test]
fn binary_header_fields_in_every_byte_order() {
    for order in ORDERS {
        let bytes = save(config(order), "", &[], true, 0);
        let h = &bytes[3200..3600];

        assert_eq!(&h[16..18], &u16_bytes(2000, order), "sample interval, {order:?}");
        assert_eq!(&h[20..22], &u16_bytes(4, order), "samples per trace, {order:?}");
        assert_eq!(&h[24..26], &u16_bytes(5, order), "data format, {order:?}");
        assert_eq!(&h[302..304], &u16_bytes(1, order), "fixed length flag, {order:?}");
        assert_eq!(&h[304..306], &u16_bytes(0, order), "extended header count, {order:?}");

        let expected_indicator: [u8; 4] = match order {
            ByteOrder::BigEndian => [0x01, 0x02, 0x03, 0x04],
            ByteOrder::LittleEndian => [0x04, 0x03, 0x02, 0x01],
            ByteOrder::SwappedWord => [0x02, 0x01, 0x04, 0x03],
        };
        assert_eq!(&h[96..100], &expected_indicator, "indicator, {order:?}");
    }
}

#[test]
fn optional_header_fields_are_written_when_given_and_zero_otherwise() {
    for order in ORDERS {
        let mut cfg = config(order);
        cfg.ensemble_fold = Some(258); // 0x0102
        cfg.trace_sorting_code = Some(4);
        cfg.measurement_system = Some(1);

        let h = &save(cfg, "", &[], true, 0)[3200..3600];
        assert_eq!(&h[26..28], &u16_bytes(258, order), "ensemble fold, {order:?}"); // 3227-3228
        assert_eq!(&h[28..30], &u16_bytes(4, order), "trace sorting code, {order:?}"); // 3229-3230
        assert_eq!(&h[54..56], &u16_bytes(1, order), "measurement system, {order:?}"); // 3255-3256

        let h = &save(config(order), "", &[], true, 0)[3200..3600];
        assert_eq!(&h[26..30], &[0, 0, 0, 0], "defaults are zero, {order:?}");
        assert_eq!(&h[54..56], &[0, 0], "defaults are zero, {order:?}");
    }
}

/// A zero byte-order field means "pre-Rev 2, big-endian". It is written as zeros.
#[test]
fn zero_byte_order_is_big_endian_and_stays_zero() {
    let mut cfg = config(ByteOrder::BigEndian);
    cfg.byte_order = 0;

    let h = &save(cfg, "", &[], true, 0)[3200..3600];
    assert_eq!(&h[96..100], &[0, 0, 0, 0]);
    assert_eq!(&h[16..18], &[0x07, 0xD0]); // fields are big-endian
}

/// A deliberate cross-check between the writer and the production parser.
#[test]
fn saved_binary_header_is_accepted_by_the_parser() {
    for order in ORDERS {
        let mut cfg = config(order);
        cfg.samples_per_trace = 291; // 0x0123, differs from its byte-swapped form
        cfg.data_format = 1;

        // 4000 characters of text -> one extended header
        let bytes = save(cfg, &pattern(4000), &[], true, 0);
        let p = parse_binary_header(&bytes[3200..3600]).unwrap();

        assert_eq!(p.sample_interval, 2000, "{order:?}");
        assert_eq!(p.samples_per_trace, 291, "{order:?}");
        assert_eq!(p.data_format.as_str(), "IBMf32", "{order:?}");
        assert_eq!(p.extended_text_header_count, 1, "{order:?}");
        assert_eq!(p.byte_order.as_str(), order.as_str(), "{order:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// Textual headers
// ---------------------------------------------------------------------------------------------

/// The main header carries up to 3120 bytes; every further 3200 bytes need one more extended
/// header.
#[test]
fn extended_header_count_at_the_boundaries() {
    // (text length, expected number of extended headers)
    let cases: [(usize, usize); 8] = [
        (0, 0),
        (1, 0),
        (3120, 0),
        (3121, 1),
        (6320, 1),
        (6321, 2),
        (9520, 2),
        (9521, 3),
    ];

    for ascii in [true, false] {
        for (len, ext) in cases {
            let bytes = save(config(ByteOrder::BigEndian), &pattern(len), &[], ascii, 0);
            assert_eq!(bytes.len(), 3600 + ext * 3200, "length {len}, ascii {ascii}");

            let count = u16::try_from(ext).unwrap().to_be_bytes();
            assert_eq!(&bytes[3200 + 304..3200 + 306], &count, "length {len}, ascii {ascii}");
        }
    }
}

#[test]
fn ascii_text_is_written_in_blocks_and_padded_with_spaces() {
    for len in [0, 10, 3120, 3121, 6320, 6321, 7000] {
        let text = pattern(len);
        let bytes = save(config(ByteOrder::BigEndian), &text, &[], true, 0);
        let blocks = expected_blocks(&text, false);

        assert_eq!(&bytes[..3200], &blocks[0][..], "main header, length {len}");
        for (i, ext) in blocks[1..].iter().enumerate() {
            let start = 3600 + i * 3200;
            assert_eq!(&bytes[start..start + 3200], &ext[..], "extended header {i}, length {len}");
        }
    }
}

#[test]
fn ebcdic_text_is_converted_and_padded_with_ebcdic_spaces() {
    for len in [0, 10, 3120, 3121, 6321, 7000] {
        let text = pattern(len);
        let bytes = save(config(ByteOrder::BigEndian), &text, &[], false, 0);
        let blocks = expected_blocks(&text, true);

        assert_eq!(&bytes[..3200], &blocks[0][..], "main header, length {len}");
        for (i, ext) in blocks[1..].iter().enumerate() {
            let start = 3600 + i * 3200;
            assert_eq!(&bytes[start..start + 3200], &ext[..], "extended header {i}, length {len}");
        }
    }
}

#[test]
fn ebcdic_header_with_letters_and_punctuation() {
    let bytes = save(config(ByteOrder::BigEndian), "C 1 SURVEY: LINE-7, 2D (TEST).", &[], false, 0);
    assert_eq!(
        &bytes[..30],
        &[
            0xC3, 0x40, 0xF1, 0x40, 0xE2, 0xE4, 0xD9, 0xE5, 0xC5, 0xE8, 0x7A, 0x40, 0xD3, 0xC9, 0xD5, 0xC5, 0x60, 0xF7,
            0x6B, 0x40, 0xF2, 0xC4, 0x40, 0x4D, 0xE3, 0xC5, 0xE2, 0xE3, 0x5D, 0x4B
        ]
    );
}

/// The reader recognises EBCDIC by looking at the last byte of the main header (0x40), so the
/// writer must always leave that byte as padding, even for text that fills the header.
#[test]
fn last_byte_of_the_main_header_is_always_padding() {
    for len in [0, 3119, 3120, 3121, 5000] {
        let text = pattern(len);
        let ascii = save(config(ByteOrder::BigEndian), &text, &[], true, 0);
        let ebcdic = save(config(ByteOrder::BigEndian), &text, &[], false, 0);
        assert_eq!(ascii[3199], 0x20, "ascii, length {len}");
        assert_eq!(ebcdic[3199], 0x40, "ebcdic, length {len}");
    }
}

/// Non-ASCII characters have no place in either encoding, but they must not panic or change the
/// size of the file.
#[test]
fn non_ascii_text_does_not_break_the_file_layout() {
    for ascii in [true, false] {
        let bytes = save(config(ByteOrder::BigEndian), "dt = 2 ms, \u{e9}chantillon \u{394}t", &[], ascii, 0);
        assert_eq!(bytes.len(), 3600, "ascii {ascii}");
    }
}

// ---------------------------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------------------------

#[test]
fn traces_get_a_minimal_header_followed_by_the_raw_samples() {
    for order in ORDERS {
        // 2 traces x (4 samples x 4 bytes), samples passed through verbatim
        let raw: Vec<u8> = (0..32).collect();
        let bytes = save(config(order), "", &raw, true, 2);

        assert_eq!(bytes.len(), 3600 + 2 * (240 + 16), "{order:?}");

        for (n, start) in [3600_usize, 3600 + 256].into_iter().enumerate() {
            let header = &bytes[start..start + 240];

            // bytes 115-116 hold the number of samples; everything else is zero
            assert_eq!(&header[114..116], &u16_bytes(4, order), "trace {n}, {order:?}");
            let others_zero = header
                .iter()
                .enumerate()
                .all(|(i, &b)| (114..116).contains(&i) || b == 0);
            assert!(others_zero, "trace {n}, {order:?}");

            assert_eq!(
                &bytes[start + 240..start + 256],
                &raw[n * 16..(n + 1) * 16],
                "trace {n} samples, {order:?}"
            );
        }
    }
}

#[test]
fn zero_traces_gives_a_headers_only_file() {
    let bytes = save(config(ByteOrder::BigEndian), "", &[], true, 0);
    assert_eq!(bytes.len(), 3600);
}

#[test]
fn samples_follow_extended_headers() {
    let raw = [0xAA_u8; 16];
    let bytes = save(config(ByteOrder::BigEndian), &pattern(4000), &raw, true, 1);

    // 3600 + 1 extended header, then 240 header bytes, then samples
    assert_eq!(bytes.len(), 3600 + 3200 + 240 + 16);
    assert_eq!(&bytes[3600 + 3200 + 240..], &raw);
}

/// `raw_traces` must hold exactly `n_traces * samples_per_trace * bytes_per_sample` bytes.
/// Currently a short buffer makes the slicing in `encode_traces` panic (a `PanicException` in
/// Python) after the output file has already been created and truncated.
#[test]
fn raw_traces_shorter_than_declared_is_an_error() {
    // 2 traces x 16 bytes = 32 needed
    assert!(!saves_ok(config(ByteOrder::BigEndian), "", &[0_u8; 31], true, 2));
}

/// Extra bytes mean `n_traces` or the geometry is wrong. Currently they are silently dropped.
/// If you would rather tolerate oversized buffers, delete this test.
#[test]
fn raw_traces_longer_than_declared_is_an_error() {
    assert!(!saves_ok(config(ByteOrder::BigEndian), "", &[0_u8; 33], true, 2));
}

#[test]
fn raw_traces_of_exactly_the_declared_size_are_accepted() {
    assert!(saves_ok(config(ByteOrder::BigEndian), "", &[0_u8; 32], true, 2));
}

// ---------------------------------------------------------------------------------------------
// Validation and I/O errors
// ---------------------------------------------------------------------------------------------

/// An invalid byte-order indicator must be rejected before anything is written.
#[test]
fn invalid_byte_order_is_rejected_without_touching_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.sgy");

    for bad in [1_u32, 0x1234_5678, 0x0403_0202, u32::MAX] {
        let mut cfg = config(ByteOrder::BigEndian);
        cfg.byte_order = bad;
        let result = save_segy(path.to_str().unwrap(), "", cfg, &[], true, 0);
        assert!(result.is_err(), "indicator {bad:#010x}");
        assert!(!path.exists(), "indicator {bad:#010x} created a file");
    }
}

#[test]
fn saving_into_a_missing_directory_is_an_error() {
    let result = save_segy("/nonexistent_dir_for_tests/out.sgy", "", config(ByteOrder::BigEndian), &[], true, 0);
    assert!(result.is_err());
}

#[test]
fn saving_overwrites_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.sgy");
    std::fs::write(&path, vec![0xFF_u8; 100_000]).unwrap();

    let result = save_segy(path.to_str().unwrap(), "", config(ByteOrder::BigEndian), &[], true, 0);
    assert!(result.is_ok());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 3600);
}
