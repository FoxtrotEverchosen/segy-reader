//! Test-only builder for SEG-Y files, assembled in memory.
//!
//! This deliberately does NOT use `encode.rs` or any other production code to write bytes. Every
//! offset comes from the SEG-Y spec tables, so a mistake shared by the reader and the writer
//! cannot hide behind a passing round-trip test.
//!

#![allow(dead_code)] // not every helper is used by every test module

use crate::types::ByteOrder;
use std::io::Write;

pub const TEXT_HEADER_LEN: usize = 3200;
pub const BINARY_HEADER_LEN: usize = 400;
pub const TRACE_HEADER_LEN: usize = 240;

// Offsets inside the 400-byte binary header: spec byte (1-based, file-relative) - 3201.
const SAMPLE_INTERVAL: usize = 3217 - 3201;
const SAMPLES_PER_TRACE: usize = 3221 - 3201;
const DATA_FORMAT: usize = 3225 - 3201;
const BYTE_ORDER_INDICATOR: usize = 3297 - 3201;
const REV_MAJOR: usize = 3501 - 3201;
const FIXED_LENGTH_FLAG: usize = 3503 - 3201;
const EXT_HEADER_COUNT: usize = 3505 - 3201;
// Offset inside the 240-byte trace header: bytes 115-116 (1-based) = samples in this trace.
const TRACE_SAMPLES: usize = 115 - 1;

#[derive(Clone)]
struct TraceSpec {
    /// What goes into trace header bytes 115-116. `None` = the binary header's sample count.
    header_samples: Option<u16>,
    /// Raw sample bytes, already encoded in the file's byte order.
    data: Vec<u8>,
}

#[derive(Clone)]
pub struct SegyBuilder {
    order: ByteOrder,
    format: i16,
    samples: u16,
    sample_interval: u16,
    rev_major: u8,
    zero_indicator: bool,
    fixed_length: bool,
    text: String,
    ebcdic: bool,
    ext_headers: Vec<String>,
    ext_count_field: Option<i16>,
    traces: Vec<TraceSpec>,
    truncate_tail: usize,
}

impl SegyBuilder {
    /// `format` is the SEG-Y data sample format code (1 = IBM f32, 5 = IEEE f32, ...).
    /// Defaults: Rev 1, 4 samples per trace, 4000 us, ASCII text, no extended headers.
    pub fn new(order: ByteOrder, format: i16) -> Self {
        Self {
            order,
            format,
            samples: 4,
            sample_interval: 4000,
            rev_major: 1,
            zero_indicator: false,
            fixed_length: false,
            text: String::new(),
            ebcdic: false,
            ext_headers: Vec::new(),
            ext_count_field: None,
            traces: Vec::new(),
            truncate_tail: 0,
        }
    }

    // ---- configuration -------------------------------------------------------------------

    /// Samples per trace as written in the *binary* header.
    pub fn samples(mut self, n: u16) -> Self {
        self.samples = n;
        self
    }

    pub fn sample_interval(mut self, us: u16) -> Self {
        self.sample_interval = us;
        self
    }

    pub fn rev(mut self, major: u8) -> Self {
        self.rev_major = major;
        self
    }

    /// Leave bytes 3297-3300 zero, as Rev 0 files do. Only valid for big-endian files.
    pub fn zero_indicator(mut self) -> Self {
        self.zero_indicator = true;
        self
    }

    pub fn fixed_length_flag(mut self) -> Self {
        self.fixed_length = true;
        self
    }

    /// Main textual header (max 3200 bytes). Padded with spaces to 3200 bytes.
    pub fn text_header(mut self, text: &str) -> Self {
        self.text = text.to_owned();
        self
    }

    /// Encode the textual headers as EBCDIC instead of ASCII (padding becomes 0x40).
    pub fn ebcdic(mut self) -> Self {
        self.ebcdic = true;
        self
    }

    /// Append one 3200-byte extended textual header. The binary header's count follows
    /// automatically unless `ext_count_field` overrides it.
    pub fn ext_header(mut self, text: &str) -> Self {
        self.ext_headers.push(text.to_owned());
        self
    }

    /// Write this value into bytes 3505-3506 regardless of how many extended headers exist.
    /// Use it to build files whose header lies.
    pub fn ext_count_field(mut self, n: i16) -> Self {
        self.ext_count_field = Some(n);
        self
    }

    /// Cut the last `n` bytes off the finished file (to simulate a truncated download).
    pub fn truncate_tail(mut self, n: usize) -> Self {
        self.truncate_tail = n;
        self
    }

    // ---- traces ---------------------------------------------------------------------------

    /// Add a trace from raw, already-encoded sample bytes. The trace header carries the binary
    /// header's sample count.
    pub fn trace_bytes(mut self, data: Vec<u8>) -> Self {
        self.traces.push(TraceSpec {
            header_samples: None,
            data,
        });
        self
    }

    /// Like `trace_bytes`, but put `header_samples` into trace header bytes 115-116
    /// (0 = "not specified, use the binary header", or a different length for ragged files).
    pub fn trace_with_header_samples(mut self, header_samples: u16, data: Vec<u8>) -> Self {
        self.traces.push(TraceSpec {
            header_samples: Some(header_samples),
            data,
        });
        self
    }

    /// f32 trace whose header carries `header_samples` in bytes 115-116 instead of the binary
    /// header's count (0 = "use the binary header", other values make a ragged file).
    pub fn trace_f32_with_header_samples(self, header_samples: u16, values: &[f32]) -> Self {
        let data = self.encode(values, f32::to_be_bytes);
        self.trace_with_header_samples(header_samples, data)
    }

    pub fn trace_f32(self, values: &[f32]) -> Self {
        let data = self.encode(values, f32::to_be_bytes);
        self.trace_bytes(data)
    }

    pub fn trace_i16(self, values: &[i16]) -> Self {
        let data = self.encode(values, i16::to_be_bytes);
        self.trace_bytes(data)
    }

    pub fn trace_i32(self, values: &[i32]) -> Self {
        let data = self.encode(values, i32::to_be_bytes);
        self.trace_bytes(data)
    }

    fn encode<T: Copy, const N: usize>(&self, values: &[T], to_be: fn(T) -> [u8; N]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|&v| file_bytes(&to_be(v), self.order))
            .collect()
    }

    // ---- output ---------------------------------------------------------------------------

    pub fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(self.text_block(&self.text));
        out.extend(self.binary_header());
        for ext in &self.ext_headers {
            out.extend(self.text_block(ext));
        }
        for t in &self.traces {
            out.extend(self.trace_header(t));
            out.extend(&t.data);
        }
        out.truncate(out.len() - self.truncate_tail);
        out
    }

    /// Writes the file to a temp location. Keep the returned handle alive for as long as the file
    /// is in use: the file is deleted when it is dropped.
    pub fn write_tmp(&self) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("create temp file");
        f.write_all(&self.build()).expect("write temp file");
        f.flush().expect("flush temp file");
        f
    }

    /// Byte offset of each trace header, computed from the actual layout (before truncation).
    pub fn trace_offsets(&self) -> Vec<u64> {
        let mut offset = TEXT_HEADER_LEN + BINARY_HEADER_LEN + self.ext_headers.len() * TEXT_HEADER_LEN;
        let mut offsets = Vec::new();
        for t in &self.traces {
            offsets.push(offset as u64);
            offset += TRACE_HEADER_LEN + t.data.len();
        }
        offsets
    }

    // ---- internals ------------------------------------------------------------------------

    /// 16-bit values: big-endian files store [hi, lo]; little-endian and swapped-word files store
    /// [lo, hi] (a swapped-word file is equivalent to little-endian for 2-byte values).
    fn u16_bytes(&self, v: u16) -> [u8; 2] {
        let [hi, lo] = v.to_be_bytes();
        match self.order {
            ByteOrder::BigEndian => [hi, lo],
            ByteOrder::LittleEndian | ByteOrder::SwappedWord => [lo, hi],
        }
    }

    fn binary_header(&self) -> [u8; BINARY_HEADER_LEN] {
        let mut h = [0_u8; BINARY_HEADER_LEN];
        let mut put16 = |at: usize, v: u16| h[at..at + 2].copy_from_slice(&self.u16_bytes(v));

        put16(SAMPLE_INTERVAL, self.sample_interval);
        put16(SAMPLES_PER_TRACE, self.samples);
        put16(DATA_FORMAT, u16::from_be_bytes(self.format.to_be_bytes()));
        put16(FIXED_LENGTH_FLAG, u16::from(self.fixed_length));
        let ext = self.ext_count_field.unwrap_or_else(|| {
            i16::try_from(self.ext_headers.len()).expect("too many extended headers for a test")
        });
        put16(EXT_HEADER_COUNT, u16::from_be_bytes(ext.to_be_bytes()));

        h[REV_MAJOR] = self.rev_major;

        if self.zero_indicator {
            assert!(
                matches!(self.order, ByteOrder::BigEndian),
                "a zero byte-order indicator means big-endian"
            );
        } else {
            let indicator = match self.order {
                ByteOrder::BigEndian => [0x01, 0x02, 0x03, 0x04],
                ByteOrder::LittleEndian => [0x04, 0x03, 0x02, 0x01],
                ByteOrder::SwappedWord => [0x02, 0x01, 0x04, 0x03],
            };
            h[BYTE_ORDER_INDICATOR..BYTE_ORDER_INDICATOR + 4].copy_from_slice(&indicator);
        }
        h
    }

    fn trace_header(&self, t: &TraceSpec) -> [u8; TRACE_HEADER_LEN] {
        let mut h = [0_u8; TRACE_HEADER_LEN];
        let n = t.header_samples.unwrap_or(self.samples);
        h[TRACE_SAMPLES..TRACE_SAMPLES + 2].copy_from_slice(&self.u16_bytes(n));
        h
    }

    fn text_block(&self, text: &str) -> Vec<u8> {
        assert!(text.len() <= TEXT_HEADER_LEN, "test text header is longer than 3200 bytes");
        let pad = if self.ebcdic { 0x40 } else { 0x20 };
        let mut block = vec![pad; TEXT_HEADER_LEN];
        for (slot, b) in block.iter_mut().zip(text.bytes()) {
            *slot = if self.ebcdic { ascii_to_ebcdic(b) } else { b };
        }
        block
    }
}

/// Takes the big-endian representation of a value and returns the bytes a file with the given
/// byte order contains for it:
///   BigEndian [A, B, C, D], LittleEndian [D, C, B, A], SwappedWord [B, A, D, C].
pub fn file_bytes(be: &[u8], order: ByteOrder) -> Vec<u8> {
    match order {
        ByteOrder::BigEndian => be.to_vec(),
        ByteOrder::LittleEndian => be.iter().rev().copied().collect(),
        ByteOrder::SwappedWord => be.chunks(2).flat_map(|pair| pair.iter().rev().copied()).collect(),
    }
}

/// Builds a textual header from card images: each line is padded (or cut) to exactly 80 columns
/// and the lines are concatenated, as in a real 40-line header.
pub fn card_lines(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l:<80.80}")).collect()
}

/// Hand-written EBCDIC (IBM-037) table for the characters tests use, independent of the `ebcdic`
/// crate the library relies on.
pub fn ascii_to_ebcdic(c: u8) -> u8 {
    match c {
        b' ' => 0x40,
        b'.' => 0x4B,
        b'(' => 0x4D,
        b'+' => 0x4E,
        b'&' => 0x50,
        b'*' => 0x5C,
        b')' => 0x5D,
        b'-' => 0x60,
        b'/' => 0x61,
        b',' => 0x6B,
        b'_' => 0x6D,
        b':' => 0x7A,
        b'#' => 0x7B,
        b'@' => 0x7C,
        b'=' => 0x7E,
        b'a'..=b'i' => 0x81 + (c - b'a'),
        b'j'..=b'r' => 0x91 + (c - b'j'),
        b's'..=b'z' => 0xA2 + (c - b's'),
        b'A'..=b'I' => 0xC1 + (c - b'A'),
        b'J'..=b'R' => 0xD1 + (c - b'J'),
        b'S'..=b'Z' => 0xE2 + (c - b'S'),
        b'0'..=b'9' => 0xF0 + (c - b'0'),
        _ => panic!("the test EBCDIC table has no entry for {c:#04x}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Self-tests: the builder is the oracle for every later test, so check it against the spec
// (literal bytes) and against the production parser (an independent code path).
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{HeaderReader, parse_binary_header};

    const ORDERS: [ByteOrder; 3] = [ByteOrder::BigEndian, ByteOrder::LittleEndian, ByteOrder::SwappedWord];

    #[test]
    fn layout_and_length() {
        let b = SegyBuilder::new(ByteOrder::BigEndian, 5)
            .samples(4)
            .ext_header("X")
            .ext_header("Y")
            .trace_f32(&[1.0, 2.0, 3.0, 4.0])
            .trace_f32(&[5.0, 6.0, 7.0, 8.0])
            .trace_f32(&[0.0; 4]);
        let bytes = b.build();

        // 3600 + 2 extended headers = 10000, then three traces of 240 + 16 bytes
        assert_eq!(bytes.len(), 3600 + 2 * 3200 + 3 * (240 + 16));
        assert_eq!(b.trace_offsets(), vec![10_000, 10_256, 10_512]);

        // text header is all spaces, extended headers start with their text, padded with spaces
        assert!(bytes[..3200].iter().all(|&x| x == 0x20));
        assert_eq!(&bytes[3600..3602], b"X ");
        assert_eq!(&bytes[6800..6802], b"Y ");

        // first sample of the first trace: 1.0f32 = 3F 80 00 00
        assert_eq!(&bytes[10_240..10_244], &[0x3F, 0x80, 0x00, 0x00]);
    }

    #[test]
    fn binary_header_agrees_with_the_parser() {
        for order in ORDERS {
            let bytes = SegyBuilder::new(order, 1)
                .samples(291)
                .sample_interval(2000)
                .ext_header("a")
                .build();

            let p = parse_binary_header(&bytes[3200..3600]).unwrap();
            assert_eq!(p.sample_interval, 2000, "{order:?}");
            assert_eq!(p.samples_per_trace, 291, "{order:?}");
            assert_eq!(p.data_format.as_str(), "IBMf32", "{order:?}");
            assert_eq!(p.extended_text_header_count, 1, "{order:?}");
            assert_eq!(p.rev_version, 1, "{order:?}");
            assert_eq!(p.byte_order.as_str(), order.as_str(), "{order:?}");
        }
    }

    #[test]
    fn revision_zero_with_zero_indicator() {
        let bytes = SegyBuilder::new(ByteOrder::BigEndian, 1)
            .rev(0)
            .zero_indicator()
            .build();
        assert_eq!(&bytes[3296..3300], &[0, 0, 0, 0]);

        let p = parse_binary_header(&bytes[3200..3600]).unwrap();
        assert_eq!(p.rev_version, 0);
        assert_eq!(p.byte_order.as_str(), "Big Endian");
    }

    #[test]
    fn ext_count_field_can_lie_about_the_file() {
        let bytes = SegyBuilder::new(ByteOrder::BigEndian, 5).ext_count_field(5).build();
        assert_eq!(bytes.len(), 3600); // no extended headers were actually written
        let p = parse_binary_header(&bytes[3200..3600]).unwrap();
        assert_eq!(p.extended_text_header_count, 5);
    }

    #[test]
    fn trace_header_sample_count_is_stored_in_file_byte_order() {
        for order in ORDERS {
            let b = SegyBuilder::new(order, 5)
                .samples(500)
                .trace_bytes(vec![]) // header carries the binary header's 500
                .trace_with_header_samples(0, vec![])
                .trace_with_header_samples(7, vec![]);
            let bytes = b.build();

            let counts: Vec<i16> = b
                .trace_offsets()
                .into_iter()
                .map(|off| {
                    let off = usize::try_from(off).unwrap();
                    HeaderReader::new(&bytes[off..off + 240], 0, order).read_i16(115)
                })
                .collect();
            assert_eq!(counts, vec![500, 0, 7], "{order:?}");
        }
    }

    #[test]
    fn typed_trace_helpers_use_the_file_byte_order() {
        let sample_bytes = |b: SegyBuilder| b.build()[3600 + 240..].to_vec();

        // f32 1.0 = 3F 80 00 00
        let f = |order| sample_bytes(SegyBuilder::new(order, 5).trace_f32(&[1.0]));
        assert_eq!(f(ByteOrder::BigEndian), [0x3F, 0x80, 0x00, 0x00]);
        assert_eq!(f(ByteOrder::LittleEndian), [0x00, 0x00, 0x80, 0x3F]);
        assert_eq!(f(ByteOrder::SwappedWord), [0x80, 0x3F, 0x00, 0x00]);

        // i16 0x0102
        let i = |order| sample_bytes(SegyBuilder::new(order, 3).trace_i16(&[0x0102]));
        assert_eq!(i(ByteOrder::BigEndian), [0x01, 0x02]);
        assert_eq!(i(ByteOrder::LittleEndian), [0x02, 0x01]);
        assert_eq!(i(ByteOrder::SwappedWord), [0x02, 0x01]);

        // i32 0x01020304
        let j = |order| sample_bytes(SegyBuilder::new(order, 2).trace_i32(&[0x0102_0304]));
        assert_eq!(j(ByteOrder::BigEndian), [0x01, 0x02, 0x03, 0x04]);
        assert_eq!(j(ByteOrder::LittleEndian), [0x04, 0x03, 0x02, 0x01]);
        assert_eq!(j(ByteOrder::SwappedWord), [0x02, 0x01, 0x04, 0x03]);
    }

    #[test]
    fn text_headers_ascii_and_ebcdic() {
        let ascii = SegyBuilder::new(ByteOrder::BigEndian, 5).text_header("C 1 TEST.").build();
        assert_eq!(&ascii[..9], b"C 1 TEST.");
        assert!(ascii[9..3200].iter().all(|&x| x == 0x20));

        let ebcdic = SegyBuilder::new(ByteOrder::BigEndian, 5)
            .text_header("C 1 TEST.")
            .ebcdic()
            .build();
        assert_eq!(&ebcdic[..9], &[0xC3, 0x40, 0xF1, 0x40, 0xE3, 0xC5, 0xE2, 0xE3, 0x4B]);
        assert!(ebcdic[9..3200].iter().all(|&x| x == 0x40));
    }

    #[test]
    fn card_lines_are_80_columns() {
        let text = card_lines(&["C 1 FIRST", "C 2 SECOND"]);
        assert_eq!(text.len(), 160);
        assert_eq!(&text[..9], "C 1 FIRST");
        assert!(text[9..80].bytes().all(|b| b == b' '));
        assert_eq!(&text[80..90], "C 2 SECOND");

        let long = card_lines(&[&"x".repeat(100)]);
        assert_eq!(long.len(), 80);
    }

    #[test]
    fn truncate_tail_and_write_tmp() {
        let b = SegyBuilder::new(ByteOrder::BigEndian, 5)
            .trace_f32(&[1.0, 2.0, 3.0, 4.0])
            .truncate_tail(10);
        assert_eq!(b.build().len(), 3600 + 240 + 16 - 10);

        let file = b.write_tmp();
        assert_eq!(std::fs::read(file.path()).unwrap(), b.build());
    }
}
