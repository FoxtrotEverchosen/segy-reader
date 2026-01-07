use std::fmt::Display;
use std::fs::File;
use std::io::ErrorKind::{InvalidInput};
use pyo3::prelude::*;
use ebcdic::ebcdic::Ebcdic;
use pyo3::exceptions::{PyIOError, PyTypeError};
use pyo3::types::{PyString, PyDict};
use numpy::{IntoPyArray};
use memmap2::{MmapOptions, Mmap};

#[pymodule]
fn _fastsegy(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SegyFile>()?;
    Ok(())
}

fn trace_to_numpy(py: Python, trace: TraceData) -> PyResult<Bound<PyAny>> {
    Ok(match trace {
        TraceData::F32(v) => v.into_pyarray(py).into_any(),
        TraceData::I16(v) => v.into_pyarray(py).into_any(),
        TraceData::I32(v) => v.into_pyarray(py).into_any(),
        TraceData::I8(v) => v.into_pyarray(py).into_any(),
    })
}

#[derive(Debug)]
struct BinaryHeader{
    sample_interval: i16,
    samples_per_trace: i16,
    bytes_per_sample: i16,
    data_format: DataFormat,
    extended_text_header_count: i16,
    byte_order: ByteOrder
}

// Only handles data formats compatible with standard <= Rev1
#[derive(Debug)]
enum DataFormat{
    IBMf32,         // Code: 1      bytes: 4
    I32,            // 2            4
    I16,            // 3            2
    FixedPointWGain,// 4            4
    IEEf32,         // 5            4
    I8,             // 8            1
}

#[derive(Debug, Copy, Clone)]
enum ByteOrder{
    BigEndian,
    LittleEndian,
    SwappedWord,
}

#[pyclass]
struct SegyFile{
    b_header: BinaryHeader,
    trace_index: Vec<u64>,
    mmap: Mmap,
    trace_count: u64,
}

#[pymethods]
impl SegyFile {
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        Self::open_segy(path)
    }

    fn get_trace<'py>(&self, py: Python<'py>, trace_number: u32) -> PyResult<Bound<'py, PyAny>> {
        let trace = match self.get_trace_data(trace_number){
            Ok(t) => t,
            Err(e) => return Err(PyTypeError::new_err(e.to_string()))
        };

        trace_to_numpy(py, trace)
    }

    fn get_metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        // TODO: Function should be called on loading file in GUI as it generates index necessary for reading traces

        let b_header: &BinaryHeader = &self.b_header;

        let dict = PyDict::new(py);
        dict.set_item("sample_interval", b_header.sample_interval)?;
        dict.set_item("samples_per_trace", b_header.samples_per_trace)?;
        dict.set_item("bytes_per_sample", b_header.bytes_per_sample)?;
        dict.set_item("extended_text_header_count", b_header.extended_text_header_count)?;

        let data_format = match b_header.data_format {
            DataFormat::IBMf32 => "IBMf32",
            DataFormat::I32 => "I32",
            DataFormat::I16 => "I16",
            DataFormat::FixedPointWGain => "FixedPointWGain",
            DataFormat::IEEf32 => "IEEf32",
            DataFormat::I8 => "I8",
        };
        dict.set_item("data_format", data_format)?;

        let byte_order = match b_header.byte_order {
            ByteOrder::BigEndian => "BigEndian",
            ByteOrder::LittleEndian => "LittleEndian",
            ByteOrder::SwappedWord => "SwappedWord",
        };
        dict.set_item("byte_order", byte_order)?;
        dict.set_item("traces", &self.trace_count)?;
        dict.set_item("index", &self.trace_index)?;
        dict.set_item("trace_count", &self.trace_count)?;

        Ok(dict)
    }

    fn get_header<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        let data = &self.mmap[..3200];

        // In All Revision standards: textual header is 3200 bytes, padded with:
        // - 0x40 (EBCDIC space) for EBCDIC encoding
        // - 0x20 (ASCII space) for ASCII encoding
        // Check last byte to determine encoding
        // This should work every time, as it is extremely unlikely for a textual header to fill all 3200 bytes
        let is_ebcdic = data[3199] == 0x40;
        let mut ascii_buf = if is_ebcdic {
            let mut result = vec![0u8; 3200];
            Ebcdic::ebcdic_to_ascii(&data, &mut result, data.len(), true, false);
            result
        } else {
            data.into()
        };

        let end = ascii_buf.iter()
            .rposition(|&b| b != 0)
            .map_or(0, |i| i + 1);

        ascii_buf = ascii_buf[..end].to_vec();

        let s = ascii_buf.chunks(80)
            .map(|line| std::str::from_utf8(line))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");

        Ok(PyString::new(py, &s))
    }
}

impl SegyFile{
    fn open_segy(path: &str) -> PyResult<Self>{
        // SAFETY:
        // While SegyFile struct exists, the read file must not be modified.
        //
        // Modifying a file after it has been opened could lead to undefined behaviour. Therefore,
        // it cannot change during lifetime of the struct

        let file = File::open(path)?;
        let mmap = unsafe {
            MmapOptions::new().map(&file)?
        };
        let b_header = match parse_binary_header(&mmap[3200..3600]){
            Ok(h) => h,
            Err(e) => return Err(PyIOError::new_err(format!("Failed to open file: {}", e)))
        };
        let (trace_count, trace_index) = match Self::build_trace_index(&b_header, &mmap){
            Ok((count, index)) => (count, index),
            Err(e) => return Err(PyErr::new::<PyTypeError, _>(e)),
        };

        Ok(Self{b_header, trace_index, mmap, trace_count})
    }

    fn build_trace_index(b_header: &BinaryHeader, mmap: &Mmap) -> Result<(u64, Vec<u64>), std::io::Error> {
        // Samples per trace read from binary header might not be correct for older data
        // Hence it might(?) be necessary to walk through whole file and count traces manually
        let mut trace_index: Vec<u64> = Vec::new();
        let mut count: u64 = 0;
        let mut offset = 3600 + b_header.extended_text_header_count as usize * 3200; //text header 3200, bin header 400

        while offset + 240 < mmap.len(){
            let samples_in_trace = read_i16(&mmap, offset+114, &b_header.byte_order);
            let samples = if samples_in_trace == 0 {
                b_header.samples_per_trace as u64
            } else {
                samples_in_trace as u64
            };

            let data_bytes = 240 + samples * b_header.bytes_per_sample as u64;
            if offset + data_bytes as usize > mmap.len(){
                break;
            }

            trace_index.push(offset as u64);
            offset += data_bytes as usize;
            count += 1;
        }

        Ok((count, trace_index))
    }

    fn get_trace_data(&self, trace_number: u32) -> Result<TraceData, SegyError> {
        let byte_order: ByteOrder = self.b_header.byte_order;
        let b_header = &self.b_header;
        let trace_index = &self.trace_index;

        if trace_number <= 0 {
            return Err(SegyError::Io(std::io::Error::new(InvalidInput, "Trace number is 1-based. Values lower than 0 are not accepted")));
        } else if trace_number > trace_index.len() as u32 {
            return Err(SegyError::TraceOutOfRange {
                requested: trace_number,
                trace_count: self.trace_index.len(),
            });
        }

        let target = trace_number - 1;

        let trace_start = if (target as usize)  < trace_index.len() {
            trace_index[target as usize]
        }else{
            return Err(SegyError::TraceOutOfRange {requested: trace_number, trace_count: trace_index.len()})
        };

        let header: &[u8] = &self.mmap[trace_start as usize .. trace_start as usize + 240];
        let samples_in_trace = read_i16(&header, 114, &b_header.byte_order);

        let samples = if samples_in_trace == 0 {
            b_header.samples_per_trace as u64
        } else {
            samples_in_trace as u64
        };

        let data_bytes = samples * b_header.bytes_per_sample as u64;
        let data_start = trace_start as usize + 240;
        let raw_buf = &self.mmap[data_start .. data_start + data_bytes as usize];

        let trace: TraceData = match b_header.data_format {
            DataFormat::IBMf32 => decode_ibm_trace(&raw_buf, &byte_order),
            DataFormat::IEEf32 => decode_ieef32_trace(&raw_buf, &byte_order),
            DataFormat::I8 => decode_i8_trace(&raw_buf),
            DataFormat::I16 => decode_i16_trace(&raw_buf, &byte_order),
            DataFormat::I32 => decode_i32_trace(&raw_buf, &byte_order),
            DataFormat::FixedPointWGain => return Err(SegyError::UnsupportedDataFormat),
        };

        Ok(trace)
    }
}

fn parse_binary_header(buf: &[u8]) -> Result<BinaryHeader, SegyError> {
    let byte_order = &buf[96..100];
    let byte_order: ByteOrder = match byte_order {
        [0x01, 0x02, 0x03, 0x04] => ByteOrder::BigEndian,
        [0x04, 0x03, 0x02, 0x01] => ByteOrder::LittleEndian,
        [0x02, 0x01, 0x04, 0x03] => ByteOrder::SwappedWord,

        // SEG-Y rev0 assumes BigEndian only
        [0x00, 0x00, 0x00, 0x00] => ByteOrder::BigEndian,
        _ => {
            return Err(SegyError::UnsupportedDataFormat);
        },
    };

    let sample_interval = read_i16(buf, 16, &byte_order);
    let data_format = read_i16(buf, 24, &byte_order);
    let samples_per_trace = read_i16(buf, 20, &byte_order);
    let extended_text_header_count = read_i16(buf, 304, &byte_order);

    let bytes_per_sample: i16 = match data_format{
        1 | 2 | 4 | 5 => 4,
        3 => 2,
        8 => 1,
        _ => return Err(SegyError::UnsupportedDataFormat)
    };

    let data_format = match data_format{
        1 => DataFormat::IBMf32,
        2 => DataFormat::I32,
        3 => DataFormat::I16,
        4 => DataFormat::FixedPointWGain,
        5 => DataFormat::IEEf32,
        8 => DataFormat::I8,
        _ => return Err(SegyError::UnsupportedDataFormat)
    };

    Ok(BinaryHeader{
        sample_interval,
        samples_per_trace,
        bytes_per_sample,
        data_format,
        extended_text_header_count,
        byte_order
    })
}

fn read_i16(buf: &[u8], offset: usize, order: &ByteOrder) -> i16 {
    let bytes = [buf[offset], buf[offset + 1]];
    match order{
        ByteOrder::BigEndian => i16::from_be_bytes(bytes),
        ByteOrder::LittleEndian => i16::from_le_bytes(bytes),
        ByteOrder::SwappedWord => i16::from_be_bytes([bytes[1], bytes[0]]),
    }
}

fn ibmf32_from_be(bytes: [u8; 4], byte_order: &ByteOrder) -> f32{
    // ibmf32 -> 1 sign bit, 7 exponent bits, 24 mantissa bits
    // unlike IEEE754 uses base 16 exponent (IEEE uses base 2)
    let word = match byte_order {
        ByteOrder::BigEndian => u32::from_be_bytes(bytes),
        ByteOrder::LittleEndian => u32::from_le_bytes(bytes),
        ByteOrder::SwappedWord => {
            u32::from_be_bytes([bytes[1], bytes[0], bytes[3], bytes[2]])
        }
    };

    if word == 0 {return 0.0;}

    let sign = if (word & 0x8000_0000) != 0 { -1.0 } else { 1.0 };
    let exponent = ((word >> 24) & 0x7F) as i32;
    let mantissa = (word & 0x00FF_FFFF) as f32 / (1 << 24) as f32;

    sign * mantissa * 16f32.powi(exponent - 64)
}

fn decode_ibm_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(4)
        .map(|b| ibmf32_from_be([b[0], b[1], b[2], b[3]], byte_order))
        .collect();

    TraceData::F32(trace_data)
}

fn ieef32_from_order(bytes: [u8; 4], byte_order: &ByteOrder) -> f32 {
    let bits = match byte_order {
        ByteOrder::LittleEndian => u32::from_le_bytes(bytes),
        ByteOrder::BigEndian => u32::from_be_bytes(bytes),
        ByteOrder::SwappedWord => u32::from_be_bytes([bytes[1], bytes[0], bytes[3], bytes[2]]),
    };

    f32::from_bits(bits)
}

fn decode_ieef32_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(4)
        .map(|b| ieef32_from_order([b[0], b[1], b[2], b[3]], byte_order))
        .collect();

    TraceData::F32(trace_data)
}

fn decode_i8_trace(data: &[u8]) -> TraceData {
    let trace = data.iter().map(|&b| b as i8).collect();

    TraceData::I8(trace)
}

fn decode_i16_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let traces = data.chunks_exact(2)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => i16::from_le_bytes([b[0], b[1]]),
            ByteOrder::BigEndian => i16::from_be_bytes([b[0], b[1]]),
            ByteOrder::SwappedWord => i16::from_be_bytes([b[1], b[0]]),
        })
        .collect();

    TraceData::I16(traces)
}

fn decode_i32_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let traces = data.chunks_exact(4)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::SwappedWord => i32::from_be_bytes([b[1], b[0], b[3], b[2]]),
        })
        .collect();

    TraceData::I32(traces)
}

enum TraceData{
    F32(Vec<f32>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I8(Vec<i8>),
}

#[derive(Debug)]
pub enum SegyError {
    Io(std::io::Error),
    TraceOutOfRange { requested: u32, trace_count: usize },
    UnsupportedDataFormat,
    CorruptTrace,
    ParseFailure,
}

impl From<std::io::Error> for SegyError {
    fn from(e: std::io::Error) -> Self {
        SegyError::Io(e)
    }
}

impl Display for SegyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            SegyError::Io(e) => e.to_string(),
            SegyError::TraceOutOfRange { requested, trace_count} => {
                String::from(format!("Trace out of range. Requested {} trace, ot ouf {} traces", requested, trace_count))
            },
            SegyError::UnsupportedDataFormat => String::from("Unsupported data format"),
            SegyError::CorruptTrace => String::from("Corrupt trace segment"),
            SegyError::ParseFailure => String::from("Failed to parse data"),
        };
        write!(f, "{:?}", result)
    }
}