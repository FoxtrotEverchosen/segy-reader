use sysinfo::System;
use std::fmt::Display;
use std::fs::File;
use std::io::ErrorKind::{InvalidInput};
use pyo3::prelude::*;
use ebcdic::ebcdic::Ebcdic;
use pyo3::exceptions::{PyIOError, PyTypeError, PyValueError};
use pyo3::types::{PyString, PyDict};
use numpy::{IntoPyArray, PyArray2};
use memmap2::{MmapOptions, Mmap};

macro_rules! build_array {
    ($py:expr, $traces:expr, $variant:path, $T:ty) => {{
        let vec2: Vec<Vec<$T>> = $traces
            .into_iter()
            .map(|t| if let $variant(v) = t { v } else { unreachable!() })
            .collect();
        PyArray2::from_vec2($py, &vec2)
            .map_err(|e| PyTypeError::new_err(format!("from_vec2 error: {:?}", e)))
            .map(|a| a.into_any())
    }};
}

#[pymodule]
fn _fastsegy(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SegyFile>()?;
    Ok(())
}

enum TraceData{
    F32(Vec<f32>),
    F64(Vec<f64>),
    I16(Vec<i16>),
    I24(Vec<i32>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    I8(Vec<i8>),
    U8(Vec<u8>),
    U16(Vec<u16>),
    U24(Vec<u32>),
    U32(Vec<u32>),
    U64(Vec<u64>),
}

#[derive(Debug)]
struct BinaryHeader{
    sample_interval: i16,
    samples_per_trace: i16,
    bytes_per_sample: i16,
    data_format: DataFormat,
    extended_text_header_count: i16,
    byte_order: ByteOrder,
    environment_type: EnvironmentType,
    dimensionality_type: DimensionalityType,
    is_time_lapsed: bool,
    layout_type: LayoutType,
}

// Only handles data formats compatible with Revision standard <= 1
#[derive(Debug)]
enum DataFormat{
    IBMf32,         // Code: 1      bytes: 4
    I32,            // 2            4
    I16,            // 3            2
    FixedPointWGain,// 4            4           (Obsolete)
    IEEf32,         // 5            4
    IEEf64,         // 6            8
    I24,            // 7            3
    I8,             // 8            1
    I64,            // 9            8
    U32,            // 10           4
    U16,            // 11           2
    U64,            // 12           8
    U24,            // 15           3
    U8,             // 16           1
}

impl DataFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataFormat::IBMf32 => "IBMf32",
            DataFormat::I32 => "I32",
            DataFormat::I16 => "I16",
            DataFormat::FixedPointWGain => "Fixed Point With Gain",
            DataFormat::IEEf32 => "IEEf32",
            DataFormat::IEEf64 => "IEEf64",
            DataFormat::I24 => "I24",
            DataFormat::I8 => "I8",
            DataFormat::I64 => "I64",
            DataFormat::U32 => "U32",
            DataFormat::U16 => "U16",
            DataFormat::U64 => "U64",
            DataFormat::U24 => "U24",
            DataFormat::U8 => "U8",
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum ByteOrder{
    BigEndian,
    LittleEndian,
    SwappedWord,
}

impl ByteOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            ByteOrder::BigEndian => "Big Endian",
            ByteOrder::LittleEndian => "Little Endian",
            ByteOrder::SwappedWord => "Swapped Word",
        }
    }
}

#[derive(Debug)]
pub enum SegyError {
    Io(std::io::Error),
    TraceOutOfRange { requested: u32, trace_count: usize },
    InvalidTraceRange {start: u32, end: u32, trace_count: usize},
    DecodingError(String),
    InvalidArgument(String),
    UnsupportedDataFormat,
    CorruptTrace,
    ParseFailure,
    RequestMemoryError,
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
                format!("Trace out of range. Requested {} trace, out of {} traces", requested, trace_count)
            },
            SegyError::InvalidTraceRange { start, end, trace_count} => {
                format!("Invalid trace range. ({start} to {end} in file with {trace_count} traces)")
            },
            SegyError::UnsupportedDataFormat => String::from("Unsupported data format"),
            SegyError::CorruptTrace => String::from("Corrupt trace segment"),
            SegyError::ParseFailure => String::from("Failed to parse data"),
            SegyError::DecodingError(e) => format!("Decoding error: {}", e),
            SegyError::RequestMemoryError => String::from("Requested data exceeds your memory limit, try with smaller trace range."),
            SegyError::InvalidArgument(e) => format!("Invalid argument: {}", e),
        };
        write!(f, "{}", result)
    }
}

impl SegyError{
    fn kind(&self) -> &str{
        match self {
            SegyError::Io(_) => "IO",
            SegyError::TraceOutOfRange{ .. } => "out_of_range",
            SegyError::InvalidTraceRange { .. } => "invalid_range",
            SegyError::UnsupportedDataFormat => "unsupported_format",
            SegyError::CorruptTrace => "corrupt_trace",
            SegyError::ParseFailure => "parse_failure",
            SegyError::RequestMemoryError => "memory_error",
            SegyError::DecodingError(_) => "decoding_error",
            SegyError::InvalidArgument(_) => "argument_error",
        }
    }
}

#[derive(Debug)]
enum EnvironmentType{
    Land,
    Marine,
    Transition,
    Downhole,
    Unspecified,
}

impl EnvironmentType {
    pub fn as_str(&self) -> &str {
        match self{
            EnvironmentType::Land => "Land",
            EnvironmentType::Marine => "Marine",
            EnvironmentType::Transition => "Transition",
            EnvironmentType::Downhole => "Downhole",
            EnvironmentType::Unspecified => "Unspecified",
        }
    }
}

#[derive(Debug)]
enum DimensionalityType{
    D1,
    D2,
    D3,
    Unspecified,
}

impl DimensionalityType {
    pub fn as_str(&self) -> &str {
        match self{
            DimensionalityType::D1 => "1D",
            DimensionalityType::D2 => "2D",
            DimensionalityType::D3 => "3D",
            DimensionalityType::Unspecified => "Unspecified",
        }
    }
}

#[derive(Debug)]
enum LayoutType{
    ParallelLines,
    CrossSpread,
    Patches,
    TowedStreamer,
    OceanBottomSensors,
    PseudoRandomSensor,
    Unspecified,
}

impl LayoutType {
    pub fn as_str(&self) -> &str {
        match self{
            LayoutType::ParallelLines => "Parallel Lines",
            LayoutType::CrossSpread => "Cross-Spread",
            LayoutType::Patches => "Patches",
            LayoutType::TowedStreamer => "Towed Streamer",
            LayoutType::OceanBottomSensors => "Ocean Bottom Sensors",
            LayoutType::PseudoRandomSensor => "Pseudo Random Sensor",
            LayoutType::Unspecified => "Unspecified",
        }
    }
}

#[pyclass]
struct SegyFile{
    b_header: BinaryHeader,
    trace_index: Vec<u64>,
    mmap: Mmap,
    trace_count: u64,
    available_mem: u64,
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
            Err(e) if e.kind() == "IO" => return Err(PyIOError::new_err(e.to_string())),
            Err(e) => return Err(PyValueError::new_err(e.to_string())),
        };

        trace_to_numpy(py, trace)
    }

    fn get_trace_range<'py>(
        &self,
        py: Python<'py>,
        start: u32,
        end: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let traces = self
            .get_trace_range_data(start, end)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        if traces.is_empty() {
            return Err(PyValueError::new_err("Got empty array"));
        }

        match &traces[0] {
            TraceData::I8(_) => build_array!(py, traces, TraceData::I8, i8),
            TraceData::I16(_) => build_array!(py, traces, TraceData::I16, i16),
            TraceData::I24(_) => build_array!(py, traces, TraceData::I24, i32),
            TraceData::I32(_) => build_array!(py, traces, TraceData::I32, i32),
            TraceData::I64(_) => build_array!(py, traces, TraceData::I64, i64),
            TraceData::U8(_) => build_array!(py, traces, TraceData::U8, u8),
            TraceData::U16(_) => build_array!(py, traces, TraceData::U16, u16),
            TraceData::U24(_) => build_array!(py, traces, TraceData::U24, u32),
            TraceData::U32(_) => build_array!(py, traces, TraceData::U32, u32),
            TraceData::U64(_) => build_array!(py, traces, TraceData::U64, u64),
            TraceData::F32(_) => build_array!(py, traces, TraceData::F32, f32),
            TraceData::F64(_) => build_array!(py, traces, TraceData::F64, f64),
        }
    }

    fn get_metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let b_header = &self.b_header;
        let dict = PyDict::new(py);

        dict.set_item("Trace Count", self.trace_count)?;
        dict.set_item("Samples Per Trace", b_header.samples_per_trace)?;
        dict.set_item("Sample Interval", b_header.sample_interval)?;
        dict.set_item("Bytes Per Sample", b_header.bytes_per_sample)?;
        dict.set_item("Data Format", b_header.data_format.as_str())?;
        dict.set_item("Environment", b_header.environment_type.as_str())?;
        dict.set_item("Dimensionality", b_header.dimensionality_type.as_str())?;
        dict.set_item("Is time lapsed", b_header.is_time_lapsed)?;
        dict.set_item("Layout", b_header.layout_type.as_str())?;

        dict.set_item("Extended Text Header Count", b_header.extended_text_header_count)?;
        dict.set_item("Byte Order", b_header.byte_order.as_str())?;

        //TODO: is it even needed here anymore?
        dict.set_item("Index", &self.trace_index)?;

        Ok(dict)
    }

    fn get_header<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        let data = &self.mmap[..3200];

        // In All Revision standards: textual header is 3200 bytes, padded with:
        // - 0x40 (EBCDIC space) for EBCDIC encoding
        // - 0x20 (ASCII space) for ASCII encoding
        // This implementation checks last byte to determine encoding
        // This should work every time, as it is extremely unlikely for a textual header to fill all 3200 bytes
        let is_ebcdic = data[3199] == 0x40;
        let mut ascii_buf = if is_ebcdic {
            let mut result = vec![0u8; 3200];
            Ebcdic::ebcdic_to_ascii(data, &mut result, data.len(), true, false);
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
        // As per memmap2 documentation All file-backed memory map constructors are marked unsafe
        // because of the potential for Undefined Behavior (UB) using the map if the underlying file
        // is subsequently modified, in or out of process.
        //
        // User modifying a file after it has been opened could lead to undefined behaviour.
        // Therefore, it cannot change during lifetime of the struct.
        //
        // It is the caller's responsibility to ensure the file remains stable and unmodified
        // during the lifetime of this `SegyFile`.

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
            Err(e) => return Err(PyIOError::new_err(format!("Failed to construct trace index: {}", e))),
        };

        let mut sys = System::new();
        sys.refresh_memory();

        //Technically this value can become stale if the process is running for a long time and
        // not represent the current state of the system accordingly.
        let available_mem = sys.available_memory();

        Ok(Self{b_header, trace_index, mmap, trace_count, available_mem})
    }

    fn build_trace_index(b_header: &BinaryHeader, mmap: &Mmap) -> Result<(u64, Vec<u64>), SegyError> {
        // Samples per trace read from binary header might not be correct for older data
        // Hence it might(?) be necessary to walk through whole file and count traces manually
        let mut trace_index: Vec<u64> = Vec::new();
        let mut count: u64 = 0;
        let mut offset = 3600 + b_header.extended_text_header_count as usize * 3200; //text header 3200, bin header 400

        while offset + 240 < mmap.len(){
            let samples_in_trace = read_i16(mmap, offset+114, &b_header.byte_order);
            let samples = if samples_in_trace <= 0 {
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

        if trace_number == 0 {
            return Err(SegyError::InvalidArgument(String::from("Trace number is 1-based. 0 is not valid")));
        } else if trace_number > trace_index.len() as u32 {
            return Err(SegyError::TraceOutOfRange {
                requested: trace_number,
                trace_count: self.trace_index.len(),
            });
        }

        let target = trace_number - 1;
        let trace_start = trace_index[target as usize];

        let header: &[u8] = &self.mmap[trace_start as usize .. trace_start as usize + 240];
        let samples_in_trace = read_i16(header, 114, &b_header.byte_order);

        let samples = if samples_in_trace <= 0 {
            b_header.samples_per_trace as u64
        } else {
            samples_in_trace as u64
        };

        let data_bytes = samples * b_header.bytes_per_sample as u64;
        let data_start = trace_start as usize + 240;
        let raw_buf = &self.mmap[data_start .. data_start + data_bytes as usize];
        let trace: TraceData = Self::decode_trace(b_header, &byte_order, raw_buf)?;

        Ok(trace)
    }

    fn get_trace_range_data(&self, start: u32, end: u32) -> Result<Vec<TraceData>, SegyError>{
        let trace_index = &self.trace_index;
        let b_header = &self.b_header;

        if start >= end {
            return Err(SegyError::Io(std::io::Error::new(
                InvalidInput,
                "Starting index must be lower than ending index"
            )))
        }

        if start == 0 || end > trace_index.len() as u32 {
            return Err(SegyError::InvalidTraceRange {
                start,
                end,
                trace_count: self.trace_index.len(),
            });
        }

        // Since data request can fetch for unlimited range of traces, it is necessary to limit how much memory can be used
        // The max will be decided based on available memory. If it is lower than 512MB that arbitrary limit will be used instead.
        //12,5%
        let soft_cap = self.available_mem / 8;
        let floor = 512 * 1024 * 1024;
        let mem_cap = soft_cap.max(floor);
        let samples_per_trace = b_header.samples_per_trace as u64;
        let bytes_per_sample = b_header.bytes_per_sample as u64;

        let total_bytes = (end - start + 1) as u64 * samples_per_trace * bytes_per_sample;
        if total_bytes > mem_cap {
            return Err(SegyError::RequestMemoryError);
        }

        let byte_order: ByteOrder = self.b_header.byte_order;
        let mut data: Vec<TraceData> = Vec::with_capacity((end - start + 1) as usize);

        for target in (start - 1) as usize ..end as usize{
            let trace_start = trace_index[target];
            let header: &[u8] = &self.mmap[trace_start as usize .. trace_start as usize + 240];
            let samples_in_trace = read_i16(header, 114, &b_header.byte_order);

            let samples = if samples_in_trace <= 0 {
                samples_per_trace
            } else {
                samples_in_trace as u64
            };

            let data_bytes = samples * b_header.bytes_per_sample as u64;
            let data_start = trace_start as usize + 240;
            let raw_buf = &self.mmap[data_start .. data_start + data_bytes as usize];

            let trace: TraceData = Self::decode_trace(b_header, &byte_order, raw_buf)?;
            data.push(trace);
        }

        Ok(data)
    }

    fn decode_trace(b_header: &BinaryHeader, byte_order: &ByteOrder, raw_buf: &[u8]) -> Result<TraceData, SegyError> {
        let trace = match b_header.data_format {
            DataFormat::IBMf32 => decode_ibm_trace(raw_buf, byte_order),
            DataFormat::IEEf32 => decode_ieef32_trace(raw_buf, byte_order),
            DataFormat::IEEf64 => decode_ieef64_trace(raw_buf, byte_order),
            DataFormat::I8 => decode_i8_trace(raw_buf),
            DataFormat::I16 => decode_i16_trace(raw_buf, byte_order),
            DataFormat::I24 => decode_i24_trace(raw_buf, byte_order)?,
            DataFormat::I32 => decode_i32_trace(raw_buf, byte_order),
            DataFormat::I64 => decode_i64_trace(raw_buf, byte_order),
            DataFormat::U8 => decode_u8_trace(raw_buf),
            DataFormat::U16 => decode_u16_trace(raw_buf, byte_order),
            DataFormat::U24 => decode_u24_trace(raw_buf, byte_order)?,
            DataFormat::U32 => decode_u32_trace(raw_buf, byte_order),
            DataFormat::U64 => decode_u64_trace(raw_buf, byte_order),
            DataFormat::FixedPointWGain => return Err(SegyError::UnsupportedDataFormat),
        };

        Ok(trace)
    }
}

fn trace_to_numpy(py: Python, trace: TraceData) -> PyResult<Bound<PyAny>> {
    Ok(match trace {
        TraceData::F32(v) => v.into_pyarray(py).into_any(),
        TraceData::F64(v) => v.into_pyarray(py).into_any(),
        TraceData::I8(v) => v.into_pyarray(py).into_any(),
        TraceData::I16(v) => v.into_pyarray(py).into_any(),
        TraceData::I24(v) => v.into_pyarray(py).into_any(),
        TraceData::I32(v) => v.into_pyarray(py).into_any(),
        TraceData::I64(v) => v.into_pyarray(py).into_any(),
        TraceData::U8(v) => v.into_pyarray(py).into_any(),
        TraceData::U16(v) => v.into_pyarray(py).into_any(),
        TraceData::U24(v) => v.into_pyarray(py).into_any(),
        TraceData::U32(v) => v.into_pyarray(py).into_any(),
        TraceData::U64(v) => v.into_pyarray(py).into_any(),
    })
}

fn parse_binary_header(buf: &[u8]) -> Result<BinaryHeader, SegyError> {
    let byte_order = &buf[96..100];
    let byte_order: ByteOrder = match byte_order {
        [0x01, 0x02, 0x03, 0x04] => ByteOrder::BigEndian,
        [0x04, 0x03, 0x02, 0x01] => ByteOrder::LittleEndian,
        [0x02, 0x01, 0x04, 0x03] => ByteOrder::SwappedWord,

        // Older SEG-Y files, following Revision 0 standard encode numbers BigEndian only, therefore
        // those files would most likely have this four bytes filled with 0
        [0x00, 0x00, 0x00, 0x00] => ByteOrder::BigEndian,
        _ => {
            return Err(SegyError::UnsupportedDataFormat);
        },
    };

    let sample_interval = read_i16(buf, 16, &byte_order);
    let data_format = read_i16(buf, 24, &byte_order);
    let samples_per_trace = read_i16(buf, 20, &byte_order);

    // TODO: Look into bytes 3521, 3529, 3513, 3509 for additional useful data
    let extended_text_header_count = read_i16(buf, 304, &byte_order);

    let bytes_per_sample: i16 = match data_format{
        8 | 16 => 1,
        3 | 11 => 2,
        7 | 15 => 3,
        1 | 2 | 4 | 5 | 10 => 4,
        6 | 9 | 12 => 8,
        _ => return Err(SegyError::UnsupportedDataFormat)
    };

    let data_format = match data_format{
        1 => DataFormat::IBMf32,
        2 => DataFormat::I32,
        3 => DataFormat::I16,
        4 => DataFormat::FixedPointWGain,
        5 => DataFormat::IEEf32,
        6 => DataFormat::IEEf64,
        7 => DataFormat::I24,
        8 => DataFormat::I8,
        9 => DataFormat::I64,
        10 => DataFormat::U32,
        11 => DataFormat::U16,
        12 => DataFormat::U64,
        15 => DataFormat::U24,
        16 => DataFormat::U8,
        _ => return Err(SegyError::UnsupportedDataFormat)
    };

    const ENVIRONMENT_MASK: i16 = 0x07;
    const DIMENSIONALITY_MASK: i16 = 0x20;
    const LAYOUT_MASK: i16 = 0x780;

    let survey_type = read_i16(buf, 310, &byte_order);
    let environment_type = match survey_type & ENVIRONMENT_MASK {
        1 => EnvironmentType::Land,
        2 => EnvironmentType::Marine,
        3 => EnvironmentType::Transition,
        4 => EnvironmentType::Downhole,
        _ => EnvironmentType::Unspecified,
    };

    let is_time_lapsed = (survey_type & 0x20) != 0;

    let dimensionality_type = match survey_type & DIMENSIONALITY_MASK {
        8 => DimensionalityType::D1,
        16 => DimensionalityType::D2,
        24 => DimensionalityType::D3,
        _ => DimensionalityType::Unspecified,
    };

    let layout_type = match survey_type & LAYOUT_MASK {
        128 => LayoutType::ParallelLines,
        256 => LayoutType::CrossSpread,
        512 => LayoutType::Patches,
        1024 => LayoutType::TowedStreamer,
        1152  => LayoutType::OceanBottomSensors,
        1280 => LayoutType::PseudoRandomSensor,
        _ => LayoutType::Unspecified,
    };

    Ok(BinaryHeader{
        sample_interval,
        samples_per_trace,
        bytes_per_sample,
        data_format,
        extended_text_header_count,
        byte_order,
        environment_type,
        is_time_lapsed,
        dimensionality_type,
        layout_type,
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

fn ibmf32_from_order(bytes: [u8; 4], byte_order: &ByteOrder) -> f32{
    // IBMf32 -> 1 sign bit, 7 exponent bits, 24 mantissa bits
    // unlike IEEE754, IBM 32-bit float uses base 16 exponent
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

fn ieef64_from_order(bytes: [u8; 8], byte_order: &ByteOrder) -> f64 {
    let bits = match byte_order {
        ByteOrder::LittleEndian => u64::from_le_bytes(bytes),
        ByteOrder::BigEndian => u64::from_be_bytes(bytes),
        ByteOrder::SwappedWord => u64::from_be_bytes([bytes[1], bytes[0], bytes[3], bytes[2], bytes[5], bytes[4], bytes[7], bytes[6]]),
    };

    f64::from_bits(bits)
}

fn decode_ieef64_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(8)
        .map(|b| ieef64_from_order([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]], byte_order))
        .collect();

    TraceData::F64(trace_data)
}

fn decode_ibm_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(4)
        .map(|b| ibmf32_from_order([b[0], b[1], b[2], b[3]], byte_order))
        .collect();

    TraceData::F32(trace_data)
}

fn decode_u8_trace(data: &[u8]) -> TraceData {
    let trace = data.to_vec();

    TraceData::U8(trace)
}

fn decode_i8_trace(data: &[u8]) -> TraceData {
    let trace = data.iter().map(|&b| b as i8).collect();

    TraceData::I8(trace)
}

fn decode_u16_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let traces = data.chunks_exact(2)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
            ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
            ByteOrder::SwappedWord => u16::from_be_bytes([b[1], b[0]]),
        })
        .collect();

    TraceData::U16(traces)
}

fn decode_u24_trace(data: &[u8], byte_order: &ByteOrder) -> Result<TraceData, SegyError> {
    let traces = data.chunks_exact(3)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => {
                Ok(u32::from_le_bytes([b[0], b[1], b[2], 0x00]))
            },
            ByteOrder::BigEndian => {
                Ok(u32::from_be_bytes([0x00, b[0], b[1], b[2]]))
            },
            ByteOrder::SwappedWord => Err(SegyError::DecodingError(String::from("Unsupported encoding: 3-byte swapped-word integer"))),
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TraceData::U24(traces))
}

fn decode_u32_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let traces = data.chunks_exact(4)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::SwappedWord => u32::from_be_bytes([b[1], b[0], b[3], b[2]]),
        })
        .collect();

    TraceData::U32(traces)
}

fn decode_u64_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let traces = data.chunks_exact(8)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::BigEndian => u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::SwappedWord => u64::from_be_bytes([b[1], b[0], b[3], b[2], b[5], b[4], b[7], b[6]]),
        })
        .collect();

    TraceData::U64(traces)
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

fn decode_i24_trace(data: &[u8], byte_order: &ByteOrder) -> Result<TraceData, SegyError> {
    let traces = data.chunks_exact(3)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => {
                let sign = if b[0] & 0x80 != 0 { 0xFF } else { 0x00 };
                Ok(i32::from_le_bytes([b[0], b[1], b[2], sign]))
            },
            ByteOrder::BigEndian => {
                let sign = if b[0] & 0x80 != 0 { 0xFF } else { 0x00 };
                Ok(i32::from_be_bytes([sign, b[0], b[1], b[2]]))
            },
            ByteOrder::SwappedWord => Err(SegyError::DecodingError(String::from("Unsupported encoding: 3-byte swapped-word integer"))),
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TraceData::I24(traces))
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

fn decode_i64_trace(data: &[u8], byte_order: &ByteOrder) -> TraceData {
    let traces = data.chunks_exact(8)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::BigEndian => i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::SwappedWord => i64::from_be_bytes([b[1], b[0], b[3], b[2], b[5], b[4], b[7], b[6]]),
        })
        .collect();

    TraceData::I64(traces)
}