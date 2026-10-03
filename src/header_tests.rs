//! Unit tests for `header.rs`.
//!
//! The 400-byte test headers are built by hand with offsets taken from the SEG-Y Rev 2.0 binary
//! header table (byte numbers are 1-based and file-relative, so `index = spec_byte - 3201`).
//! Writing the offsets as `3217 - 3201` keeps every constant traceable to the table.

use super::*;

const ORDERS: [ByteOrder; 3] = [ByteOrder::BigEndian, ByteOrder::LittleEndian, ByteOrder::SwappedWord];

const SAMPLE_INTERVAL: usize = 3217 - 3201;
const ORIG_SAMPLE_INTERVAL: usize = 3219 - 3201;
const SAMPLES_PER_TRACE: usize = 3221 - 3201;
const ORIG_SAMPLES_PER_TRACE: usize = 3223 - 3201;
const DATA_FORMAT: usize = 3225 - 3201;
const ENSEMBLE_FOLD: usize = 3227 - 3201;
const BYTE_ORDER_INDICATOR: usize = 3297 - 3201;
const REV_MAJOR: usize = 3501 - 3201;
const REV_MINOR: usize = 3502 - 3201;
const EXT_HEADER_COUNT: usize = 3505 - 3201;

/// A 400-byte binary header under construction.
///
/// `new` fills in a valid header (IEEE f32, 1000 samples, 4000 us) plus "decoy" values in the
/// fields next to the ones the parser reads, so a read at the wrong offset returns a wrong value
/// instead of silently reading zeros.
struct Hdr {
    buf: [u8; 400],
    order: ByteOrder,
}

impl Hdr {
    fn new(order: ByteOrder) -> Self {
        let mut h = Self { buf: [0; 400], order };

        let indicator = match order {
            ByteOrder::BigEndian => [0x01, 0x02, 0x03, 0x04],
            ByteOrder::LittleEndian => [0x04, 0x03, 0x02, 0x01],
            ByteOrder::SwappedWord => [0x02, 0x01, 0x04, 0x03],
        };
        h.buf[BYTE_ORDER_INDICATOR..BYTE_ORDER_INDICATOR + 4].copy_from_slice(&indicator);

        h.set_i16(SAMPLE_INTERVAL, 4000)
            .set_i16(SAMPLES_PER_TRACE, 1000)
            .set_i16(DATA_FORMAT, 5);

        h.set_i16(ORIG_SAMPLE_INTERVAL, 0x1111)
            .set_i16(ORIG_SAMPLES_PER_TRACE, 0x2222)
            .set_i16(ENSEMBLE_FOLD, 0x3333);
        h
    }

    /// 16-bit fields: big-endian file -> [hi, lo]; little-endian and swapped-word -> [lo, hi]
    /// (a swapped-word file is equivalent to little-endian for 2-byte values).
    fn set_u16(&mut self, at: usize, v: u16) -> &mut Self {
        let [hi, lo] = v.to_be_bytes();
        let bytes = match self.order {
            ByteOrder::BigEndian => [hi, lo],
            ByteOrder::LittleEndian | ByteOrder::SwappedWord => [lo, hi],
        };
        self.buf[at..at + 2].copy_from_slice(&bytes);
        self
    }

    fn set_i16(&mut self, at: usize, v: i16) -> &mut Self {
        self.set_u16(at, u16::from_be_bytes(v.to_be_bytes()))
    }

    /// Single bytes are not affected by byte order.
    fn set_u8(&mut self, at: usize, v: u8) -> &mut Self {
        self.buf[at] = v;
        self
    }

    fn parse(&self) -> Result<BinaryHeader, SegyError> {
        parse_binary_header(&self.buf)
    }
}

// ---------------------------------------------------------------------------------------------
// Field offsets and byte order
// ---------------------------------------------------------------------------------------------

/// Values are chosen so each one differs from its byte-swapped form
/// (2000 = 0x07D0, 291 = 0x0123, 3 = 0x0003), so reading with the wrong byte order fails the test.
#[test]
fn parses_every_field_in_every_byte_order() {
    for order in ORDERS {
        let mut h = Hdr::new(order);
        h.set_i16(SAMPLE_INTERVAL, 2000)
            .set_i16(SAMPLES_PER_TRACE, 291)
            .set_i16(DATA_FORMAT, 1)
            .set_i16(EXT_HEADER_COUNT, 3)
            .set_u8(REV_MAJOR, 1);

        let p = h.parse().unwrap();

        assert_eq!(p.sample_interval, 2000, "{order:?}");
        assert_eq!(p.samples_per_trace, 291, "{order:?}");
        assert_eq!(p.data_format.as_str(), "IBMf32", "{order:?}");
        assert_eq!(p.bytes_per_sample, 4, "{order:?}");
        assert_eq!(p.extended_text_header_count, 3, "{order:?}");
        assert_eq!(p.rev_version, 1, "{order:?}");
        assert_eq!(p.byte_order.as_str(), order.as_str(), "{order:?}");

        // Survey-type bytes are zero in this header.
        assert_eq!(p.environment_type.as_str(), "Unspecified");
        assert_eq!(p.dimensionality_type.as_str(), "Unspecified");
        assert_eq!(p.layout_type.as_str(), "Unspecified");
        assert!(!p.is_time_lapsed);
    }
}

#[test]
fn byte_order_indicator_accepted_values() {
    // (indicator bytes as stored in the file, byte order the fields are written in, expected)
    let cases: [([u8; 4], ByteOrder, &str); 4] = [
        ([0x01, 0x02, 0x03, 0x04], ByteOrder::BigEndian, "Big Endian"),
        ([0x00, 0x00, 0x00, 0x00], ByteOrder::BigEndian, "Big Endian"), // Rev 0/1: field unused
        ([0x04, 0x03, 0x02, 0x01], ByteOrder::LittleEndian, "Little Endian"),
        ([0x02, 0x01, 0x04, 0x03], ByteOrder::SwappedWord, "Swapped Word"),
    ];

    for (indicator, field_order, expected) in cases {
        let mut h = Hdr::new(field_order);
        h.buf[BYTE_ORDER_INDICATOR..BYTE_ORDER_INDICATOR + 4].copy_from_slice(&indicator);

        let p = h.parse().unwrap();
        assert_eq!(p.byte_order.as_str(), expected, "indicator {indicator:02X?}");
        assert_eq!(p.samples_per_trace, 1000, "indicator {indicator:02X?}");
    }
}

/// Pins current behaviour: any other value in bytes 3297-3300 rejects the file.
///
/// Those bytes are only defined from Rev 2 on. A Rev 0/1 file could legally hold anything there,
/// and this parser would refuse it. 
#[test]
fn byte_order_indicator_unrecognised_values_are_rejected() {
    let rejected: [[u8; 4]; 6] = [
        [0x01, 0x02, 0x03, 0x05],
        [0x00, 0x00, 0x00, 0x01],
        [0x01, 0x00, 0x00, 0x00],
        [0x03, 0x04, 0x01, 0x02],
        [0x04, 0x03, 0x02, 0x00],
        [0xFF, 0xFF, 0xFF, 0xFF],
    ];

    for indicator in rejected {
        let mut h = Hdr::new(ByteOrder::BigEndian);
        h.buf[BYTE_ORDER_INDICATOR..BYTE_ORDER_INDICATOR + 4].copy_from_slice(&indicator);
        assert!(h.parse().is_err(), "indicator {indicator:02X?} should be rejected");
    }
}

// ---------------------------------------------------------------------------------------------
// Revision
// ---------------------------------------------------------------------------------------------

#[test]
fn revision_0_and_1_are_accepted() {
    for order in ORDERS {
        for rev in [0_u8, 1] {
            let mut h = Hdr::new(order);
            h.set_u8(REV_MAJOR, rev);
            assert_eq!(h.parse().unwrap().rev_version, rev, "{order:?}");
        }
    }
}

#[test]
fn revision_2_and_above_are_rejected() {
    for order in ORDERS {
        for rev in [2_u8, 3, 0x10, 0xFF] {
            let mut h = Hdr::new(order);
            h.set_u8(REV_MAJOR, rev);
            assert!(h.parse().is_err(), "rev {rev}, {order:?}");
        }
    }
}

/// Major and minor revision are two separate 8-bit fields (bytes 3501 and 3502), so the major
/// number is the byte at 3501 regardless of byte order, and the minor byte is not looked at.
#[test]
fn revision_is_two_independent_bytes() {
    for order in ORDERS {
        for minor in [0x00_u8, 0x01, 0xFF] {
            let mut h = Hdr::new(order);
            h.set_u8(REV_MAJOR, 1).set_u8(REV_MINOR, minor);
            assert_eq!(h.parse().unwrap().rev_version, 1, "minor {minor:#04x}, {order:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Extended textual header count
// ---------------------------------------------------------------------------------------------

#[test]
fn extended_header_count_valid_values() {
    for order in ORDERS {
        for n in [0_i16, 1, 7, 32_767] {
            let mut h = Hdr::new(order);
            h.set_i16(EXT_HEADER_COUNT, n);
            let got = h.parse().unwrap().extended_text_header_count;
            assert_eq!(got, usize::try_from(n).unwrap(), "{order:?}");
        }
    }
}

/// -1 means "variable number, terminated by an `EndText` stanza" in the spec. That is not
/// supported, and it must be reported as an error, not a panic or a huge allocation.
#[test]
fn extended_header_count_negative_is_rejected() {
    for order in ORDERS {
        for n in [-1_i16, -2, i16::MIN] {
            let mut h = Hdr::new(order);
            h.set_i16(EXT_HEADER_COUNT, n);
            assert!(h.parse().is_err(), "count {n}, {order:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Data sample format code
// ---------------------------------------------------------------------------------------------

#[test]
fn data_format_codes_map_to_format_and_sample_size() {
    // (code, bytes per sample, DataFormat::as_str)
    let table: [(i16, usize, &str); 14] = [
        (1, 4, "IBMf32"),
        (2, 4, "I32"),
        (3, 2, "I16"),
        (4, 4, "Fixed Point With Gain"), // parses; decoding it is rejected later
        (5, 4, "IEEf32"),
        (6, 8, "IEEf64"),
        (7, 3, "I24"),
        (8, 1, "I8"),
        (9, 8, "I64"),
        (10, 4, "U32"),
        (11, 2, "U16"),
        (12, 8, "U64"),
        (15, 3, "U24"),
        (16, 1, "U8"),
    ];

    for order in ORDERS {
        for (code, size, name) in table {
            let mut h = Hdr::new(order);
            h.set_i16(DATA_FORMAT, code);
            let p = h.parse().unwrap();
            assert_eq!(p.bytes_per_sample, size, "code {code}, {order:?}");
            assert_eq!(p.data_format.as_str(), name, "code {code}, {order:?}");
        }
    }
}

#[test]
fn undefined_data_format_codes_are_rejected() {
    for code in [0_i16, 13, 14, 17, 100, -1, i16::MIN, i16::MAX] {
        let mut h = Hdr::new(ByteOrder::BigEndian);
        h.set_i16(DATA_FORMAT, code);
        assert!(h.parse().is_err(), "code {code}");
    }
}

#[test]
fn all_zero_header_is_rejected() {
    // Format code 0 is invalid, so a zero-filled header cannot be mistaken for a valid file.
    assert!(parse_binary_header(&[0_u8; 400]).is_err());
}

// ---------------------------------------------------------------------------------------------
// Samples per trace
// ---------------------------------------------------------------------------------------------

#[test]
fn samples_per_trace_in_i16_range() {
    // 0 is valid: the real count then comes from each trace header.
    for order in ORDERS {
        for n in [0_i16, 1, 32_767] {
            let mut h = Hdr::new(order);
            h.set_i16(SAMPLES_PER_TRACE, n);
            let got = h.parse().unwrap().samples_per_trace;
            assert_eq!(got, usize::try_from(n).unwrap(), "{order:?}");
        }
    }
}

/// Number of samples cannot be negative, so values of 32768 and above are record lengths, not
/// negative numbers. Rev 2.0 section 3.3 lists "number of samples per trace" among the fields that
/// are exempt from the two's-complement rule, and long records are common in practice.
#[test]
fn samples_per_trace_above_i16_max_is_read_as_unsigned() {
    for order in ORDERS {
        for n in [32_768_u16, 40_000, 65_535] {
            let mut h = Hdr::new(order);
            h.set_u16(SAMPLES_PER_TRACE, n);
            let got = h.parse().unwrap().samples_per_trace;
            assert_eq!(got, usize::from(n), "{order:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// HeaderReader
// ---------------------------------------------------------------------------------------------

#[test]
fn header_reader_maps_one_based_file_bytes_to_buffer_indices() {
    let buf: Vec<u8> = (0..=255_u8).collect(); // buf[i] == i

    // Binary header: base 3200, so file byte 3201 is buffer index 0.
    let binary = HeaderReader::new(&buf, 3200, ByteOrder::BigEndian);
    assert_eq!(binary.read_u8(3201), 0);
    assert_eq!(binary.read_u8(3217), 16); // sample interval
    assert_eq!(binary.read_u8(3456), 255);

    // Trace header: base 0, so byte 1 is index 0 and bytes 115-116 sit at indices 114-115.
    let trace = HeaderReader::new(&buf, 0, ByteOrder::BigEndian);
    assert_eq!(trace.read_u8(1), 0);
    assert_eq!(trace.read_u8(115), 114);
}

#[test]
fn header_reader_i16_byte_order() {
    let buf = [0x12, 0x34];
    let be = HeaderReader::new(&buf, 0, ByteOrder::BigEndian).read_i16(1);
    let le = HeaderReader::new(&buf, 0, ByteOrder::LittleEndian).read_i16(1);
    let sw = HeaderReader::new(&buf, 0, ByteOrder::SwappedWord).read_i16(1);
    assert_eq!(be, 0x1234);
    assert_eq!(le, 0x3412);
    assert_eq!(sw, 0x3412);
}

#[test]
fn header_reader_i16_is_signed() {
    let buf = [0xFF, 0xFE];
    assert_eq!(HeaderReader::new(&buf, 0, ByteOrder::BigEndian).read_i16(1), -2);
    assert_eq!(HeaderReader::new(&buf, 0, ByteOrder::LittleEndian).read_i16(1), -257); // 0xFEFF
}

#[test]
fn header_reader_u64_byte_order() {
    let buf = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    let be = HeaderReader::new(&buf, 0, ByteOrder::BigEndian).read_u64(1);
    let le = HeaderReader::new(&buf, 0, ByteOrder::LittleEndian).read_u64(1);
    let sw = HeaderReader::new(&buf, 0, ByteOrder::SwappedWord).read_u64(1);
    assert_eq!(be, 0x0102_0304_0506_0708);
    assert_eq!(le, 0x0807_0605_0403_0201);
    assert_eq!(sw, 0x0201_0403_0605_0807);
}

/// Trace header bytes 115-116 (1-based) hold the number of samples in the trace.
#[test]
fn header_reader_finds_trace_sample_count_at_bytes_115_to_116() {
    let mut trace_header = [0_u8; 240];
    trace_header[114] = 0x01;
    trace_header[115] = 0xF4; // 0x01F4 = 500

    let be = HeaderReader::new(&trace_header, 0, ByteOrder::BigEndian).read_i16(115);
    assert_eq!(be, 500);

    trace_header[114] = 0xF4;
    trace_header[115] = 0x01;
    let le = HeaderReader::new(&trace_header, 0, ByteOrder::LittleEndian).read_i16(115);
    assert_eq!(le, 500);
}
