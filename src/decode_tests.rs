//! Unit tests for `decode.rs`.
//!
//! Expected values are hand-computed from the IBM / IEEE-754 / two's-complement definitions,
//! never produced by the decoder itself. The only helper that "knows" about byte orders is
//! `to_file_bytes`, and it is deliberately written differently from the decoder (reverse / swap
//! pairs of a big-endian representation) so a shared mistake is unlikely.

#![allow(clippy::float_cmp)]

use super::*;

const ORDERS: [ByteOrder; 3] = [ByteOrder::BigEndian, ByteOrder::LittleEndian, ByteOrder::SwappedWord];

/// Takes the big-endian representation of a value and returns the bytes a SEG-Y file with the
/// given byte order would contain for it.
///   - `BigEndian`:    [A, B, C, D]
///   - `LittleEndian`: [D, C, B, A]
///   - `SwappedWord`:  [B, A, D, C]  (each 2-byte pair reversed)
fn to_file_bytes(be: &[u8], order: ByteOrder) -> Vec<u8> {
    match order {
        ByteOrder::BigEndian => be.to_vec(),
        ByteOrder::LittleEndian => be.iter().rev().copied().collect(),
        ByteOrder::SwappedWord => be.chunks(2).flat_map(|pair| pair.iter().rev().copied()).collect(),
    }
}

fn bytes4(be: &[u8], order: ByteOrder) -> [u8; 4] {
    to_file_bytes(be, order).try_into().unwrap()
}

fn bytes8(be: &[u8], order: ByteOrder) -> [u8; 8] {
    to_file_bytes(be, order).try_into().unwrap()
}

/// `TraceData` has no `Debug`/`PartialEq`, so unwrap the expected variant or panic.
macro_rules! expect_variant {
    ($e:expr, $variant:path) => {
        match $e {
            $variant(v) => v,
            _ => panic!("unexpected TraceData variant"),
        }
    };
}

// ---------------------------------------------------------------------------------------------
// IBM 32-bit float
// ---------------------------------------------------------------------------------------------
// value = (-1)^sign * (mantissa / 2^24) * 16^(exponent - 64)
// All of these are exactly representable in f32, so exact equality is correct.

/// (IBM word written big-endian, expected value)
const IBM_VECTORS: &[(u32, f32)] = &[
    (0x4264_0000, 100.0),
    (0xC276_A000, -118.625), // the classic textbook example
    (0x4276_A000, 118.625),
    (0x4110_0000, 1.0),
    (0xC110_0000, -1.0),
    (0x4080_0000, 0.5),
    (0x4210_0000, 16.0),
    (0x4010_0000, 0.0625),
    (0x4101_0000, 0.0625), // same value, un-normalised mantissa
    (0x0000_0000, 0.0),
];

#[test]
fn ibm_known_vectors_big_endian() {
    for &(word, expected) in IBM_VECTORS {
        let got = ibmf32_from_order(word.to_be_bytes(), ByteOrder::BigEndian);
        assert_eq!(got, expected, "word {word:#010x}");
    }
}

#[test]
fn ibm_known_vectors_all_byte_orders() {
    for order in ORDERS {
        for &(word, expected) in IBM_VECTORS {
            let got = ibmf32_from_order(bytes4(&word.to_be_bytes(), order), order);
            assert_eq!(got, expected, "word {word:#010x}, order {order:?}");
        }
    }
}

#[test]
fn ibm_byte_order_literals() {
    // 100.0 = 0x42640000 in big-endian
    assert_eq!(ibmf32_from_order([0x42, 0x64, 0x00, 0x00], ByteOrder::BigEndian), 100.0);
    assert_eq!(ibmf32_from_order([0x00, 0x00, 0x64, 0x42], ByteOrder::LittleEndian), 100.0);
    assert_eq!(ibmf32_from_order([0x64, 0x42, 0x00, 0x00], ByteOrder::SwappedWord), 100.0);
}

#[test]
fn ibm_zero_mantissa_is_zero_regardless_of_sign_and_exponent() {
    for word in [0x0000_0000_u32, 0x8000_0000, 0x4200_0000, 0xC200_0000, 0x5F00_0000] {
        let got = ibmf32_from_order(word.to_be_bytes(), ByteOrder::BigEndian);
        assert_eq!(got, 0.0, "word {word:#010x}");
    }
}

/// Zero mantissa with an exponent >= 96: `16^(e-64)` overflows f32 to +inf, and `0.0 * inf` is NaN.
/// An un-normalised zero is rare in real files, but it must not turn into NaN.
#[test]
fn ibm_zero_mantissa_with_huge_exponent_is_zero_not_nan() {
    for word in [0x6000_0000_u32, 0x7F00_0000, 0xFF00_0000] {
        let got = ibmf32_from_order(word.to_be_bytes(), ByteOrder::BigEndian);
        assert_eq!(got, 0.0, "word {word:#010x} decoded to {got}");
    }
}

#[test]
fn ibm_large_value_just_below_f32_overflow() {
    // exponent 0x5F = 95, mantissa 1/16  ->  16^(31) / 16 = 16^30 = 2^120
    let got = ibmf32_from_order(0x5F10_0000_u32.to_be_bytes(), ByteOrder::BigEndian);
    assert_eq!(got, 2f32.powi(120));
}

/// IBM's range (~7e75) is far wider than f32's (~3e38). Pin the behaviour for out-of-range values:
/// they saturate to +/- infinity.
#[test]
fn ibm_values_beyond_f32_range_become_infinity() {
    let big = ibmf32_from_order(0x7FFF_FFFF_u32.to_be_bytes(), ByteOrder::BigEndian);
    let neg = ibmf32_from_order(0xFFFF_FFFF_u32.to_be_bytes(), ByteOrder::BigEndian);
    assert_eq!(big, f32::INFINITY);
    assert_eq!(neg, f32::NEG_INFINITY);
}

/// Smallest IBM exponent: ~16^-64 = 2^-256, far below the smallest f32 subnormal (2^-149).
#[test]
fn ibm_values_below_f32_range_flush_to_zero() {
    let got = ibmf32_from_order(0x00FF_FFFF_u32.to_be_bytes(), ByteOrder::BigEndian);
    assert_eq!(got, 0.0);
}

#[test]
fn decode_ibm_trace_multiple_samples_all_orders() {
    let words: [u32; 4] = [0x4264_0000, 0xC276_A000, 0x0000_0000, 0x4080_0000];
    let expected = vec![100.0_f32, -118.625, 0.0, 0.5];

    for order in ORDERS {
        let data: Vec<u8> = words.iter().flat_map(|w| to_file_bytes(&w.to_be_bytes(), order)).collect();
        let got = expect_variant!(decode_ibm_trace(&data, order), TraceData::F32);
        assert_eq!(got, expected, "order {order:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// IEEE-754 f32
// ---------------------------------------------------------------------------------------------

#[test]
fn ieee_f32_known_vectors_literal_bytes() {
    // 1.0 = 0x3F800000
    assert_eq!(ieef32_from_order([0x3F, 0x80, 0x00, 0x00], ByteOrder::BigEndian), 1.0);
    assert_eq!(ieef32_from_order([0x00, 0x00, 0x80, 0x3F], ByteOrder::LittleEndian), 1.0);
    assert_eq!(ieef32_from_order([0x80, 0x3F, 0x00, 0x00], ByteOrder::SwappedWord), 1.0);

    // -2.5 = 0xC0200000
    assert_eq!(ieef32_from_order([0xC0, 0x20, 0x00, 0x00], ByteOrder::BigEndian), -2.5);
    assert_eq!(ieef32_from_order([0x00, 0x00, 0x20, 0xC0], ByteOrder::LittleEndian), -2.5);
    assert_eq!(ieef32_from_order([0x20, 0xC0, 0x00, 0x00], ByteOrder::SwappedWord), -2.5);
}

#[test]
fn ieee_f32_preserves_exact_bit_patterns() {
    let patterns: [u32; 8] = [
        0x0000_0000, // +0
        0x8000_0000, // -0
        0x0000_0001, // smallest subnormal
        0x007F_FFFF, // largest subnormal
        0x7F7F_FFFF, // f32::MAX
        0x7F80_0000, // +inf
        0xFF80_0000, // -inf
        0x7FC0_0001, // quiet NaN with payload
    ];

    for order in ORDERS {
        for bits in patterns {
            let got = ieef32_from_order(bytes4(&bits.to_be_bytes(), order), order);
            assert_eq!(got.to_bits(), bits, "bits {bits:#010x}, order {order:?}");
        }
    }
}

#[test]
fn decode_ieef32_trace_multiple_samples_all_orders() {
    let expected = vec![1.0_f32, -2.5, 0.0, 1e-10, f32::MAX, f32::MIN_POSITIVE];

    for order in ORDERS {
        let data: Vec<u8> = expected
            .iter()
            .flat_map(|v| to_file_bytes(&v.to_be_bytes(), order))
            .collect();
        let got = expect_variant!(decode_ieef32_trace(&data, order), TraceData::F32);
        assert_eq!(got, expected, "order {order:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// IEEE-754 f64
// ---------------------------------------------------------------------------------------------

#[test]
fn ieee_f64_known_vectors_literal_bytes() {
    // 1.0 = 0x3FF0000000000000
    assert_eq!(
        ieef64_from_order([0x3F, 0xF0, 0, 0, 0, 0, 0, 0], ByteOrder::BigEndian),
        1.0
    );
    assert_eq!(
        ieef64_from_order([0, 0, 0, 0, 0, 0, 0xF0, 0x3F], ByteOrder::LittleEndian),
        1.0
    );
    assert_eq!(
        ieef64_from_order([0xF0, 0x3F, 0, 0, 0, 0, 0, 0], ByteOrder::SwappedWord),
        1.0
    );

    // -2.5 = 0xC004000000000000
    assert_eq!(
        ieef64_from_order([0xC0, 0x04, 0, 0, 0, 0, 0, 0], ByteOrder::BigEndian),
        -2.5
    );
    assert_eq!(
        ieef64_from_order([0, 0, 0, 0, 0, 0, 0x04, 0xC0], ByteOrder::LittleEndian),
        -2.5
    );
    assert_eq!(
        ieef64_from_order([0x04, 0xC0, 0, 0, 0, 0, 0, 0], ByteOrder::SwappedWord),
        -2.5
    );
}

#[test]
fn ieee_f64_preserves_exact_bit_patterns() {
    let patterns: [u64; 7] = [
        0x0000_0000_0000_0000, // +0
        0x8000_0000_0000_0000, // -0
        0x0000_0000_0000_0001, // smallest subnormal
        0x7FEF_FFFF_FFFF_FFFF, // f64::MAX
        0x7FF0_0000_0000_0000, // +inf
        0xFFF0_0000_0000_0000, // -inf
        0x7FF8_0000_0000_0001, // quiet NaN with payload
    ];

    for order in ORDERS {
        for bits in patterns {
            let got = ieef64_from_order(bytes8(&bits.to_be_bytes(), order), order);
            assert_eq!(got.to_bits(), bits, "bits {bits:#018x}, order {order:?}");
        }
    }
}

#[test]
fn decode_ieef64_trace_multiple_samples_all_orders() {
    let expected = vec![1.0_f64, -2.5, 0.0, 1e-300, f64::MAX];

    for order in ORDERS {
        let data: Vec<u8> = expected
            .iter()
            .flat_map(|v| to_file_bytes(&v.to_be_bytes(), order))
            .collect();
        let got = expect_variant!(decode_ieef64_trace(&data, order), TraceData::F64);
        assert_eq!(got, expected, "order {order:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// 8-bit integers (no byte order)
// ---------------------------------------------------------------------------------------------

#[test]
fn u8_is_passthrough() {
    let data = [0x00, 0x01, 0x7F, 0x80, 0xFF];
    let got = expect_variant!(decode_u8_trace(&data), TraceData::U8);
    assert_eq!(got, data.to_vec());
}

#[test]
fn i8_is_twos_complement() {
    let data = [0x00, 0x01, 0x7F, 0x80, 0xFF];
    let got = expect_variant!(decode_i8_trace(&data), TraceData::I8);
    assert_eq!(got, vec![0, 1, 127, -128, -1]);
}

// ---------------------------------------------------------------------------------------------
// 16 / 32 / 64-bit integers
// ---------------------------------------------------------------------------------------------

/// Boundary values round-trip through every byte order. `to_file_bytes` is the oracle here.
macro_rules! int_boundary_test {
    ($test:ident, $decode:ident, $variant:path, $ty:ty) => {
        #[test]
        fn $test() {
            let expected: Vec<$ty> = vec![
                <$ty>::MIN,
                <$ty>::MIN.wrapping_add(1),
                0,
                1,
                <$ty>::MAX - 1,
                <$ty>::MAX,
            ];
            for order in ORDERS {
                let data: Vec<u8> = expected
                    .iter()
                    .flat_map(|v| to_file_bytes(&v.to_be_bytes(), order))
                    .collect();
                let got = expect_variant!($decode(&data, order), $variant);
                assert_eq!(got, expected, "order {order:?}");
            }
        }
    };
}

int_boundary_test!(i16_boundaries_all_orders, decode_i16_trace, TraceData::I16, i16);
int_boundary_test!(u16_boundaries_all_orders, decode_u16_trace, TraceData::U16, u16);
int_boundary_test!(i32_boundaries_all_orders, decode_i32_trace, TraceData::I32, i32);
int_boundary_test!(u32_boundaries_all_orders, decode_u32_trace, TraceData::U32, u32);
int_boundary_test!(i64_boundaries_all_orders, decode_i64_trace, TraceData::I64, i64);
int_boundary_test!(u64_boundaries_all_orders, decode_u64_trace, TraceData::U64, u64);

/// Literal byte positions, so the tests above are not the only evidence for byte-order handling.
#[test]
fn i16_byte_positions_literal() {
    let data = [0x01, 0x02];
    let be = expect_variant!(decode_i16_trace(&data, ByteOrder::BigEndian), TraceData::I16);
    let le = expect_variant!(decode_i16_trace(&data, ByteOrder::LittleEndian), TraceData::I16);
    let sw = expect_variant!(decode_i16_trace(&data, ByteOrder::SwappedWord), TraceData::I16);
    assert_eq!(be, vec![0x0102]);
    assert_eq!(le, vec![0x0201]);
    // For 16-bit values a swapped-word file is equivalent to little-endian.
    assert_eq!(sw, vec![0x0201]);
}

#[test]
fn i32_byte_positions_literal() {
    let data = [0x01, 0x02, 0x03, 0x04];
    let be = expect_variant!(decode_i32_trace(&data, ByteOrder::BigEndian), TraceData::I32);
    let le = expect_variant!(decode_i32_trace(&data, ByteOrder::LittleEndian), TraceData::I32);
    let sw = expect_variant!(decode_i32_trace(&data, ByteOrder::SwappedWord), TraceData::I32);
    assert_eq!(be, vec![0x0102_0304]);
    assert_eq!(le, vec![0x0403_0201]);
    assert_eq!(sw, vec![0x0201_0403]);
}

#[test]
fn u64_byte_positions_literal() {
    let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    let be = expect_variant!(decode_u64_trace(&data, ByteOrder::BigEndian), TraceData::U64);
    let le = expect_variant!(decode_u64_trace(&data, ByteOrder::LittleEndian), TraceData::U64);
    let sw = expect_variant!(decode_u64_trace(&data, ByteOrder::SwappedWord), TraceData::U64);
    assert_eq!(be, vec![0x0102_0304_0506_0708]);
    assert_eq!(le, vec![0x0807_0605_0403_0201]);
    assert_eq!(sw, vec![0x0201_0403_0605_0807]);
}

// ---------------------------------------------------------------------------------------------
// 24-bit integers
// ---------------------------------------------------------------------------------------------

#[test]
fn i24_sign_extension_big_and_little_endian() {
    // (big-endian bytes, expected)
    let cases: [([u8; 3], i32); 7] = [
        ([0x00, 0x00, 0x00], 0),
        ([0x01, 0x02, 0x03], 66_051),
        ([0x7F, 0xFF, 0xFF], 8_388_607),  // i24::MAX
        ([0x80, 0x00, 0x00], -8_388_608), // i24::MIN
        ([0x80, 0x00, 0x01], -8_388_607),
        ([0xFF, 0xFF, 0xFF], -1),
        ([0xFF, 0xFF, 0xFE], -2),
    ];

    for (be, expected) in cases {
        let got = expect_variant!(decode_i24_trace(&be, ByteOrder::BigEndian).unwrap(), TraceData::I24);
        assert_eq!(got, vec![expected], "BE bytes {be:02X?}");

        let mut le = be;
        le.reverse();
        let got = expect_variant!(decode_i24_trace(&le, ByteOrder::LittleEndian).unwrap(), TraceData::I24);
        assert_eq!(got, vec![expected], "LE bytes {le:02X?}");
    }
}

#[test]
fn i24_multiple_samples() {
    let data = [0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x01, 0x80, 0x00, 0x00];
    let got = expect_variant!(
        decode_i24_trace(&data, ByteOrder::BigEndian).unwrap(),
        TraceData::I24
    );
    assert_eq!(got, vec![-1, 1, -8_388_608]);
}

#[test]
fn u24_has_no_sign_extension_big_and_little_endian() {
    let cases: [([u8; 3], u32); 5] = [
        ([0x00, 0x00, 0x00], 0),
        ([0x01, 0x02, 0x03], 66_051),
        ([0x7F, 0xFF, 0xFF], 8_388_607),
        ([0x80, 0x00, 0x00], 8_388_608),
        ([0xFF, 0xFF, 0xFF], 16_777_215), // u24::MAX
    ];

    for (be, expected) in cases {
        let got = expect_variant!(decode_u24_trace(&be, ByteOrder::BigEndian).unwrap(), TraceData::U24);
        assert_eq!(got, vec![expected], "BE bytes {be:02X?}");

        let mut le = be;
        le.reverse();
        let got = expect_variant!(decode_u24_trace(&le, ByteOrder::LittleEndian).unwrap(), TraceData::U24);
        assert_eq!(got, vec![expected], "LE bytes {le:02X?}");
    }
}

#[test]
fn swapped_word_is_unsupported_for_24_bit() {
    let data = [0x01, 0x02, 0x03];
    assert!(matches!(
        decode_i24_trace(&data, ByteOrder::SwappedWord),
        Err(SegyError::DecodingError(_))
    ));
    assert!(matches!(
        decode_u24_trace(&data, ByteOrder::SwappedWord),
        Err(SegyError::DecodingError(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// Edge-case inputs
// ---------------------------------------------------------------------------------------------

#[test]
fn empty_input_gives_empty_trace_for_every_decoder() {
    let empty: &[u8] = &[];
    let o = ByteOrder::BigEndian;

    assert!(expect_variant!(decode_ibm_trace(empty, o), TraceData::F32).is_empty());
    assert!(expect_variant!(decode_ieef32_trace(empty, o), TraceData::F32).is_empty());
    assert!(expect_variant!(decode_ieef64_trace(empty, o), TraceData::F64).is_empty());
    assert!(expect_variant!(decode_u8_trace(empty), TraceData::U8).is_empty());
    assert!(expect_variant!(decode_i8_trace(empty), TraceData::I8).is_empty());
    assert!(expect_variant!(decode_u16_trace(empty, o), TraceData::U16).is_empty());
    assert!(expect_variant!(decode_i16_trace(empty, o), TraceData::I16).is_empty());
    assert!(expect_variant!(decode_u24_trace(empty, o).unwrap(), TraceData::U24).is_empty());
    assert!(expect_variant!(decode_i24_trace(empty, o).unwrap(), TraceData::I24).is_empty());
    assert!(expect_variant!(decode_u32_trace(empty, o), TraceData::U32).is_empty());
    assert!(expect_variant!(decode_i32_trace(empty, o), TraceData::I32).is_empty());
    assert!(expect_variant!(decode_u64_trace(empty, o), TraceData::U64).is_empty());
    assert!(expect_variant!(decode_i64_trace(empty, o), TraceData::I64).is_empty());
}

#[test]
fn input_shorter_than_one_sample_gives_empty_trace() {
    let o = ByteOrder::BigEndian;
    assert!(expect_variant!(decode_ieef32_trace(&[0x3F, 0x80, 0x00], o), TraceData::F32).is_empty());
    assert!(expect_variant!(decode_i16_trace(&[0x01], o), TraceData::I16).is_empty());
    assert!(expect_variant!(decode_ibm_trace(&[0x42], o), TraceData::F32).is_empty());
}
