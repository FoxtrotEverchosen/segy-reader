use ebcdic::ebcdic::Ebcdic;
use memmap2::{Mmap, MmapOptions};
use numpy::{IntoPyArray, PyArray2};
use pyo3::exceptions::{PyIOError, PyTypeError, PyValueError};
use pyo3::prelude::PyDictMethods;
use pyo3::types::{PyDict, PyString};
use pyo3::{Bound, PyAny, PyResult, Python, pyclass, pymethods};
use std::fs::File;
use std::io::ErrorKind::InvalidInput;
use sysinfo::System;

use crate::decode::{
    decode_i8_trace, decode_i16_trace, decode_i24_trace, decode_i32_trace, decode_i64_trace, decode_ibm_trace,
    decode_ieef32_trace, decode_ieef64_trace, decode_u8_trace, decode_u16_trace, decode_u24_trace, decode_u32_trace,
    decode_u64_trace,
};
use crate::header::{BinaryHeader, HeaderReader, parse_binary_header};
use crate::types::{ByteOrder, DataFormat, SegyError, TraceData};

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

#[pyclass]
pub struct SegyFile {
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
        let trace = match self.get_trace_data(trace_number) {
            Ok(t) => t,
            Err(e) if e.kind() == "IO" => return Err(PyIOError::new_err(e.to_string())),
            Err(e) => return Err(PyValueError::new_err(e.to_string())),
        };

        Ok(trace_to_numpy(py, trace))
    }

    fn get_trace_range<'py>(&self, py: Python<'py>, start: u32, end: u32) -> PyResult<Bound<'py, PyAny>> {
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
        dict.set_item("Revision Standard", b_header.rev_version)?;

        Ok(dict)
    }

    fn get_header<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        let data = &self.mmap[..3200];
        let ext_count = self.b_header.extended_text_header_count;
        let ext_data: Option<&[u8]> = if ext_count > 0 {
            let d = &self.mmap[3600..3600 + ext_count * 3200];
            Some(d)
        } else {
            None
        };

        // In All Revision standards: textual header is 3200 bytes, padded with:
        // - 0x40 (EBCDIC space) for EBCDIC encoding
        // - 0x20 (ASCII space) for ASCII encoding
        // This implementation checks last byte to determine encoding
        // This should work every time, as it is extremely unlikely for a textual header to fill all 3200 bytes
        let is_ebcdic = data[3199] == 0x40;
        let ascii_buf = if is_ebcdic {
            let mut result = vec![0u8; 3200];
            Ebcdic::ebcdic_to_ascii(data, &mut result, data.len(), true, false);

            if let Some(ext_data) = ext_data {
                let mut ext_result = vec![0u8; ext_count * 3200];
                Ebcdic::ebcdic_to_ascii(ext_data, &mut ext_result, ext_data.len(), true, false);
                result.extend_from_slice(&ext_result);
            }
            result
        } else {
            let mut result: Vec<u8> = data.into();
            if let Some(ext_data) = ext_data {
                result.extend_from_slice(ext_data);
            }
            result
        };

        let s = ascii_buf
            .chunks(80)
            .map(|line| std::str::from_utf8(line))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");

        Ok(PyString::new(py, &s))
    }
}

impl SegyFile {
    fn open_segy(path: &str) -> PyResult<Self> {
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
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let b_header = match parse_binary_header(&mmap[3200..3600]) {
            Ok(h) => h,
            Err(e) => return Err(PyIOError::new_err(format!("Failed to open file: {e}"))),
        };
        let (trace_count, trace_index) = Self::build_trace_index(&b_header, &mmap);

        let mut sys = System::new();
        sys.refresh_memory();

        // This value could become stale if the process is running for a long time and
        // not represent the current state of the system accordingly.
        let available_mem = sys.available_memory();

        Ok(Self {
            b_header,
            trace_index,
            mmap,
            trace_count,
            available_mem,
        })
    }

    fn build_trace_index(b_header: &BinaryHeader, mmap: &Mmap) -> (u64, Vec<u64>) {
        // Samples per trace read from binary header might not be correct for older data
        // Hence it might(?) be necessary to walk through whole file and count traces manually
        let mut trace_index: Vec<u64> = Vec::new();
        let mut count: u64 = 0;

        //text header 3200, bin header 400
        let mut offset = 3600 + b_header.extended_text_header_count * 3200;
        while offset + 240 < mmap.len() {
            let reader = HeaderReader::new(&mmap[offset..offset + 240], 0, b_header.byte_order);

            // This reads sample count from TRACE header, which *should* be more accurate
            let samples_in_trace =
                usize::try_from(reader.read_i16(115)).expect("Sample count in trace cannot be negative");
            let samples = if samples_in_trace == 0 {
                b_header.samples_per_trace
            } else {
                samples_in_trace
            };

            let data_bytes = 240 + samples * b_header.bytes_per_sample;
            if offset + data_bytes > mmap.len() {
                break;
            }

            trace_index.push(offset as u64);
            offset += data_bytes;
            count += 1;
        }

        (count, trace_index)
    }

    fn get_trace_data(&self, trace_number: u32) -> Result<TraceData, SegyError> {
        let byte_order: ByteOrder = self.b_header.byte_order;
        let b_header = &self.b_header;
        let trace_index = &self.trace_index;

        if trace_number == 0 {
            return Err(SegyError::InvalidArgument(String::from("Trace number is 1-based. 0 is not valid")));
        }

        // Most files should not include more than 2^16 traces. In rare cases that value could reach up to 2^32 as per SEG-Y documentation
        if trace_number
            > u32::try_from(trace_index.len())
                .expect("It should be impossible for file to contain more than 2^32 traces")
        {
            return Err(SegyError::TraceOutOfRange {
                requested: trace_number,
                trace_count: self.trace_index.len(),
            });
        }

        let target = trace_number - 1;

        let trace_start = usize::try_from(trace_index[target as usize])
            .expect("Mmap should have failed before any offset could exceed usize::MAX");

        let reader = HeaderReader::new(&self.mmap[trace_start..trace_start + 240], 0, b_header.byte_order);
        let samples_in_trace = usize::try_from(reader.read_i16(115)).expect("Sample count in trace cannot be negative");
        let samples = if samples_in_trace == 0 {
            b_header.samples_per_trace
        } else {
            samples_in_trace
        };

        let data_bytes = samples * b_header.bytes_per_sample;
        let data_start = trace_start + 240;
        let raw_buf = &self.mmap[data_start..data_start + data_bytes];
        let trace: TraceData = Self::decode_trace(b_header, byte_order, raw_buf)?;

        Ok(trace)
    }

    fn get_trace_range_data(&self, start: u32, end: u32) -> Result<Vec<TraceData>, SegyError> {
        let trace_index = &self.trace_index;
        let b_header = &self.b_header;

        if start >= end {
            return Err(SegyError::Io(std::io::Error::new(
                InvalidInput,
                "Starting index must be lower than ending index",
            )));
        }
        if start == 0 || end as usize > trace_index.len() {
            return Err(SegyError::InvalidTraceRange {
                start,
                end,
                trace_count: self.trace_index.len(),
            });
        }

        // Since data request can fetch for unlimited range of traces, it is necessary to limit how much memory can be used
        // The max will be decided based on available memory. If it is lower than 512MB that arbitrary limit will be used instead.
        // 12,5%
        let soft_cap = self.available_mem / 8;
        let mem_cap = soft_cap.max(512 * 1024 * 1024);
        let trace_count = (end - start + 1) as usize;
        let total_bytes = trace_count * b_header.samples_per_trace * b_header.bytes_per_sample;

        if total_bytes as u64 > mem_cap {
            return Err(SegyError::RequestMemoryError);
        }

        let byte_order = self.b_header.byte_order;

        ((start - 1) as usize..end as usize)
            .map(|target| {
                let trace_start = usize::try_from(trace_index[target])
                    .expect("mmap would have failed before offset exceeds usize::MAX");

                let reader = HeaderReader::new(&self.mmap[trace_start..trace_start + 240], 0, b_header.byte_order);
                let samples_in_trace =
                    usize::try_from(reader.read_i16(115)).expect("samples in trace cannot be negative");

                let samples = if samples_in_trace == 0 {
                    b_header.samples_per_trace
                } else {
                    samples_in_trace
                };

                let data_bytes = samples * b_header.bytes_per_sample;
                let data_start = trace_start + 240;
                let raw_buf = &self.mmap[data_start..data_start + data_bytes];

                Self::decode_trace(b_header, byte_order, raw_buf)
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn decode_trace(b_header: &BinaryHeader, byte_order: ByteOrder, raw_buf: &[u8]) -> Result<TraceData, SegyError> {
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

#[allow(clippy::match_same_arms)]
pub fn trace_to_numpy(py: Python, trace: TraceData) -> Bound<PyAny> {
    macro_rules! convert {
        ($v:expr) => {
            $v.into_pyarray(py).into_any()
        };
    }

    match trace {
        TraceData::F32(v) => convert!(v),
        TraceData::F64(v) => convert!(v),
        TraceData::I8(v) => convert!(v),
        TraceData::I16(v) => convert!(v),
        TraceData::I24(v) => convert!(v),
        TraceData::I32(v) => convert!(v),
        TraceData::I64(v) => convert!(v),
        TraceData::U8(v) => convert!(v),
        TraceData::U16(v) => convert!(v),
        TraceData::U24(v) => convert!(v),
        TraceData::U32(v) => convert!(v),
        TraceData::U64(v) => convert!(v),
    }
}
