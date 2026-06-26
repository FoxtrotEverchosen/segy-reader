use std::fs::{File};
use std::io::{BufWriter, Write};
use ebcdic::ebcdic::Ebcdic;
use pyo3::{pyclass, pyfunction, pymethods, PyErr, PyResult};
use pyo3::exceptions::PyValueError;
use crate::types::{ByteOrder};

// There exists a discrepancy between types stored by the HeaderConfig and types stored by
// BinaryHeader struct that represent the same data. That is a deliberate decision, since SEGY docs
// require the data to be encoded as specific data type (majorly i16 with exceptions).
#[pyclass]
#[derive(Clone, Copy)]
pub struct BinaryHeaderConfig {
    // Mandatory fields             // byte location (0-based) in 400 byte wide header
    #[pyo3(get, set)]
    sample_interval: i16,           // 16-17
    #[pyo3(get, set)]
    samples_per_trace: i16,         // 20-21
    #[pyo3(get, set)]
    data_format: i16,               // 24-25
    #[pyo3(get, set)]
    revision_number: i16,           // 300-301
    #[pyo3(get, set)]
    fixed_length: i16,              // 302-303
    #[pyo3(get, set)]
    byte_order: u32,                // 97-300
    #[pyo3(get, set)]
    bytes_per_sample: usize,        // not encoded


    // Highly recommended fields
    #[pyo3(get, set)]
    ensemble_fold: Option<i16>,     // 26-27
    #[pyo3(get, set)]
    trace_sorting_code: Option<i16>,// 28-29
    #[pyo3(get, set)]
    measurement_system: Option<i16>,// 54-55
}

#[pymethods]
impl BinaryHeaderConfig {
    #[new]
    #[pyo3(signature = (sample_interval, samples_per_trace, data_format, revision_number, fixed_length, byte_order, bytes_per_sample, ensemble_fold=None, measurement_system=None, trace_sorting_code=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(sample_interval: i16,
           samples_per_trace: i16,
           data_format: i16,
           revision_number: i16,
           fixed_length: i16,
           byte_order: u32,
           bytes_per_sample: usize,
           ensemble_fold: Option<i16>,
           measurement_system: Option<i16>,
           trace_sorting_code: Option<i16>) -> Self
    {
        BinaryHeaderConfig {
            sample_interval,
            samples_per_trace,
            data_format,
            revision_number,
            fixed_length,
            byte_order,
            bytes_per_sample,
            ensemble_fold,
            trace_sorting_code,
            measurement_system,
        }
    }
}

#[pyfunction]
// takes numpy array
pub fn save_segy(file_path: &str, textual_header: &str, b_header_config: BinaryHeaderConfig, raw_traces: &[u8], is_ascii: bool, n_traces: usize, n_samples: usize) -> PyResult<()> {
    // All textual headers are not stored in one place. As per Rev >= 1.0, the main textual header
    // is stored in bytes 1 - 3200. Extended text headers are stored after binary header, i.e. bytes
    // 3600 - N * 3200, where N is the number of extended text headers provided in bin_header

    let byte_order: ByteOrder = match b_header_config.byte_order {
        // Older SEG-Y files, following Revision 0 standard encode numbers BigEndian only, therefore
        // those files would most likely have this four bytes filled with 0
        0x01_02_03_04 | 0x00_00_00_00 => ByteOrder::BigEndian,
        0x04_03_02_01 => ByteOrder::LittleEndian,
        0x02_01_04_03 => ByteOrder::SwappedWord,
        _ => {
            return Err(PyErr::new::<PyValueError, _>("End user provided wrong byte_order"));
        }
    };

    // create() truncates file if it exists, informing user of overriding file should be done via GUI window
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);
    let mut ext_header_count: i16 = 0;

    // last row left for padding
    if textual_header.len() > 3120 {
        ext_header_count = ((textual_header.len() as f64 - 3120.0) / 3200.0).ceil() as i16;
    }

    encode_txt_header(is_ascii, textual_header, &mut writer)?;
    encode_bin_header(b_header_config, &mut writer, byte_order, ext_header_count)?;

    if ext_header_count > 0 {
        encode_ext_txt_header(is_ascii, textual_header, &mut writer, ext_header_count)?;
    }

    encode_traces(b_header_config, raw_traces, n_traces, n_samples, &mut writer, byte_order)?;

    writer.flush()?;
    Ok(())
}

fn encode_bin_header(conf: BinaryHeaderConfig, writer: &mut BufWriter<File>, byte_order: ByteOrder, ext_header_count: i16) -> Result<(), PyErr> {
    let mut header = vec![0u8;400];

    let i16_bytes = |v: i16| match byte_order {
        ByteOrder::BigEndian    => v.to_be_bytes(),
        // for 16-bit le is equivalent to swapped word
        ByteOrder::LittleEndian | ByteOrder::SwappedWord => v.to_le_bytes(),
    };

    header[16..18].copy_from_slice(&i16_bytes(conf.sample_interval));
    header[20..22].copy_from_slice(&i16_bytes(conf.samples_per_trace));
    header[24..26].copy_from_slice(&i16_bytes(conf.data_format));
    header[300..302].copy_from_slice(&i16_bytes(conf.revision_number));
    header[302..304].copy_from_slice(&i16_bytes(conf.fixed_length));
    header[304..306].copy_from_slice(&i16_bytes(ext_header_count));

    // The byte order indicator is always written in big-endian regardless of the file's byte order
    header[96..100].copy_from_slice(&conf.byte_order.to_be_bytes());

    header[26..28].copy_from_slice(&i16_bytes(conf.ensemble_fold.unwrap_or(0x00)));
    header[28..30].copy_from_slice(&i16_bytes(conf.trace_sorting_code.unwrap_or(0x00)));
    header[54..56].copy_from_slice(&i16_bytes(conf.measurement_system.unwrap_or(0x00)));

    writer.write_all(&header)?;
    Ok(())
}

fn encode_txt_header(is_ascii: bool, header: &str, writer: &mut BufWriter<File>) -> Result<(), PyErr>{
    let padding = if is_ascii { 0x20u8 } else { 0x40u8 };
    let mut buf = [padding; 3200];

    let src = if is_ascii{
        header.as_bytes().to_vec()
    }else{
        let mut ebcdic = vec![0u8; header.len()];
        Ebcdic::ascii_to_ebcdic(header.as_bytes(), &mut ebcdic, header.len(), true);
        ebcdic
    };

    let len = src.len().min(3120);
    buf[..len].copy_from_slice(&src[..len]);
    writer.write_all(&buf)?;

    Ok(())
}

fn encode_ext_txt_header(is_ascii: bool, header: &str, writer: &mut BufWriter<File>, ext_header_count: i16) -> Result<(), PyErr>{
    let padding = if is_ascii { 0x20u8 } else { 0x40u8 };

    for i in 0 ..ext_header_count{
        let start = usize::try_from(3120 + i * 3200)
            .expect("This value will never be negative");
        let end = (start + 3200).min(header.len());
        let slice = &header.as_bytes()[start..end];

        let mut buf = [padding; 3200];

        let src = if is_ascii{
            slice.to_vec()
        }else{
            let mut ebcdic = vec![padding; 3200];
            Ebcdic::ascii_to_ebcdic(slice, &mut ebcdic, slice.len(), true);
            ebcdic
        };

        let len = src.len().min(3200);
        buf[..len].copy_from_slice(&src[..len]);
        writer.write_all(&buf)?;
    }

    Ok(())
}

fn encode_traces(conf: BinaryHeaderConfig, raw_traces: &[u8], n_traces: usize, n_samples: usize, writer: &mut BufWriter<File>, byte_order: ByteOrder) -> PyResult<()> {
    // Number of samples in trace -> bytes 114 - 115 (0-based)

    let bytes_per_sample = conf.bytes_per_sample;
    let trace_data_size = n_samples * bytes_per_sample;

    let i16_bytes = |v: i16| match byte_order {
        ByteOrder::BigEndian    => v.to_be_bytes(),
        // for 16-bit le is equivalent to swapped word
        ByteOrder::LittleEndian | ByteOrder::SwappedWord => v.to_le_bytes(),
    };

    let samples_per_trace = i16_bytes(conf.samples_per_trace);

    // for each trace:
    for i in 0 .. n_traces {
        // encode trace header (240 bytes)
        let mut buf_header = [0u8; 240];
        buf_header[114..116].copy_from_slice(&samples_per_trace);
        writer.write_all(&buf_header)?;

        // encode trace data (X bytes)
        let trace_bytes = &raw_traces[i * trace_data_size..(i + 1) * trace_data_size];
        writer.write_all(trace_bytes)?;
    }

    Ok(())
}