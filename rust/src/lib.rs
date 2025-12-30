use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use pyo3::prelude::*;
use ebcdic::ebcdic::Ebcdic;
use pyo3::types::PyString;
use byteorder::{ ReadBytesExt, BigEndian};

#[pymodule]
fn fastsegy(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hello, m)?)?;
    m.add_function(wrap_pyfunction!(get_header, m)?)?;
    m.add_function(wrap_pyfunction!(get_metadata, m)?)?;
    Ok(())
}

#[pyfunction]
fn hello() -> &'static str {
    "Hello from Rust!"
}

#[pyfunction]
fn get_header(py: Python) -> PyResult<Bound<PyString>> {
    let mut f = File::open("C:/fastsegy/rust/Kerry3D.segy")?;
    let mut buf = vec![0u8; 3200]; //header should always be of size 3200 bytes
    f.read_exact(&mut buf)?;

    // In All Revision standards: textual header is 3200 bytes, padded with:
    // - 0x40 (EBCDIC space) for EBCDIC encoding
    // - 0x20 (ASCII space) for ASCII encoding
    // Check last byte to determine encoding
    // This should work every time, as it is extremely unlikely for a textual header to fill all 3200 bytes
    let is_ebcdic = buf[3199] == 0x40;
    let mut ascii_buf = if is_ebcdic {
        let mut result = vec![0u8; 3200];
        Ebcdic::ebcdic_to_ascii(&buf, &mut result, buf.len(), true, false);
        result
    } else {
        buf
    };

    let end = ascii_buf.iter()
        .rposition(|&b| b != 0)
        .map_or(0, |i| i + 1);

    ascii_buf = ascii_buf[..end].to_vec();

    let s = ascii_buf.chunks(80)
        .map(|line| std::str::from_utf8(line))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");

    Ok(PyString::new_bound(py, &s))
}

#[pyfunction]
fn get_metadata(py: Python) -> PyResult<()> {
    let mut f = File::open("C:/fastsegy/rust/Kerry3D.segy")?;
    let mut buf = [0u8; 400]; //binary header should always be of size 400 bytes
    f.seek(SeekFrom::Start(3200))?;
    f.read_exact(&mut buf)?;

    let b_header: BinaryHeader = parse_binary_header(&buf);

    println!("{:#?}", b_header);

    let traces = approx_trace_number(&b_header, "C:/fastsegy/rust/Kerry3D.segy");
    println!("how many traces: {}", traces);
    let actual_traces = count_traces(&b_header,"C:/fastsegy/rust/Kerry3D.segy")?;
    println!("calculated traces: {}", actual_traces);
    Ok(())
}

#[derive(Debug)]
struct BinaryHeader{
    traces_per_record: i16,
    sample_interval: i16,
    samples_per_trace: i16,
    bytes_per_sample: i16,
    data_format: DataFormat,
    extended_text_headers: i16,
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

#[derive(Debug)]
enum ByteOrder{
    BigEndian,
    LittleEndian,
    SwappedWord,
}

// TODO: Improve error handling in rust part. Remove unwraps or switch to expect if possible,
// TODO: Don't propagate all errors out of Rusts scope
// TODO: Remove hardcoded paths!

fn parse_binary_header(buf: &[u8; 400]) -> BinaryHeader {
    let byte_order = &buf[96..100];
    let byte_order: ByteOrder = match byte_order {
        [0x01, 0x02, 0x03, 0x04] => ByteOrder::BigEndian,
        [0x04, 0x03, 0x02, 0x01] => ByteOrder::LittleEndian,
        [0x02, 0x01, 0x04, 0x03] => ByteOrder::SwappedWord,

        // SEG-Y rev0 assumes BigEndian only
        [0x00, 0x00, 0x00, 0x00] => ByteOrder::BigEndian,
        _ => {
            panic!("Unknown SEG-Y byte order in file binary header")
        },
    };

    let traces_per_record = read_i16(buf, 12, &byte_order);
    let sample_interval = read_i16(buf, 16, &byte_order);
    let data_format = read_i16(buf, 24, &byte_order);
    let samples_per_trace = read_i16(buf, 20, &byte_order);
    let extended_text_headers = read_i16(buf, 304, &byte_order);

    let bytes_per_sample: i16 = match data_format{
        1 | 2 | 4 | 5 => 4,
        3 => 2,
        8 => 1,
        _ => 4
    };

    let data_format = match data_format{
        1 => DataFormat::IBMf32,
        2 => DataFormat::I32,
        3 => DataFormat::I16,
        4 => DataFormat::FixedPointWGain,
        5 => DataFormat::IEEf32,
        8 => DataFormat::I8,
        _ => panic!("Tried to parse unknown data format") // a temporary(?) solution
    };

    BinaryHeader{
        traces_per_record,
        sample_interval,
        samples_per_trace,
        bytes_per_sample,
        data_format,
        extended_text_headers,
        byte_order
    }
}

fn read_i16(buf: &[u8], offset: usize, order: &ByteOrder) -> i16 {
    let bytes = [buf[offset], buf[offset + 1]];
    match order{
        ByteOrder::BigEndian => i16::from_be_bytes(bytes),
        ByteOrder::LittleEndian => i16::from_le_bytes(bytes),
        ByteOrder::SwappedWord => i16::from_be_bytes([bytes[1], bytes[0]]),
    }
}

fn approx_trace_number(b_header: &BinaryHeader, path: &str) -> u64 {
    let filesize: u64 = fs::metadata(path).unwrap().len();
    let data_bytes: u64 = filesize - 3600;

    let trace_size: u64 = 240 + b_header.bytes_per_sample as u64 * b_header.samples_per_trace as u64;
    data_bytes / trace_size
}

fn count_traces(b_header: &BinaryHeader, path: &str) -> Result<u64, std::io::Error> {
    let mut file = File::open(path)?;
    let start: u64 = 3600 + b_header.extended_text_headers as u64 * 3200;

    file.seek(SeekFrom::Start(start))?;
    let mut count: u64 = 0;

    loop {
        let mut header = [0u8; 240];

        match file.read_exact(&mut header) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }

        // Bytes 115 and 116 of trace header hold the numbers of samples in current trace
        let samples_in_trace = u16::from_be_bytes([
            header[114],
            header[115],
        ]);

        let samples = if samples_in_trace == 0 {
            b_header.samples_per_trace as u64
        } else {
            samples_in_trace as u64
        };

        let data_bytes = samples * b_header.bytes_per_sample as u64;
        file.seek(SeekFrom::Current(data_bytes as i64))?;
        count += 1;
    }
    Ok(count)
}

//TODO: parse trace data based on used data format