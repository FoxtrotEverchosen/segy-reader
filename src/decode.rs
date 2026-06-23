use crate::types::{ByteOrder, SegyError, TraceData};

#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_wrap)]
pub fn ibmf32_from_order(bytes: [u8; 4], byte_order: ByteOrder) -> f32{
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

pub fn ieef32_from_order(bytes: [u8; 4], byte_order: ByteOrder) -> f32 {
    let bits = match byte_order {
        ByteOrder::LittleEndian => u32::from_le_bytes(bytes),
        ByteOrder::BigEndian => u32::from_be_bytes(bytes),
        ByteOrder::SwappedWord => u32::from_be_bytes([bytes[1], bytes[0], bytes[3], bytes[2]]),
    };

    f32::from_bits(bits)
}

pub fn decode_ieef32_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(4)
        .map(|b| ieef32_from_order([b[0], b[1], b[2], b[3]], byte_order))
        .collect();

    TraceData::F32(trace_data)
}

pub fn ieef64_from_order(bytes: [u8; 8], byte_order: ByteOrder) -> f64 {
    let bits = match byte_order {
        ByteOrder::LittleEndian => u64::from_le_bytes(bytes),
        ByteOrder::BigEndian => u64::from_be_bytes(bytes),
        ByteOrder::SwappedWord => u64::from_be_bytes([bytes[1], bytes[0], bytes[3], bytes[2], bytes[5], bytes[4], bytes[7], bytes[6]]),
    };

    f64::from_bits(bits)
}

pub fn decode_ieef64_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(8)
        .map(|b| ieef64_from_order([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]], byte_order))
        .collect();

    TraceData::F64(trace_data)
}

pub fn decode_ibm_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let trace_data = data
        .chunks_exact(4)
        .map(|b| ibmf32_from_order([b[0], b[1], b[2], b[3]], byte_order))
        .collect();

    TraceData::F32(trace_data)
}

pub fn decode_u8_trace(data: &[u8]) -> TraceData {
    let trace = data.to_vec();

    TraceData::U8(trace)
}

pub fn decode_i8_trace(data: &[u8]) -> TraceData {
    let trace = data.iter().map(|&b| b.cast_signed()).collect();

    TraceData::I8(trace)
}

pub fn decode_u16_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let traces = data.chunks_exact(2)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
            ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
            ByteOrder::SwappedWord => u16::from_be_bytes([b[1], b[0]]),
        })
        .collect();

    TraceData::U16(traces)
}

pub fn decode_u24_trace(data: &[u8], byte_order: ByteOrder) -> Result<TraceData, SegyError> {
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

pub fn decode_u32_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let traces = data.chunks_exact(4)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::SwappedWord => u32::from_be_bytes([b[1], b[0], b[3], b[2]]),
        })
        .collect();

    TraceData::U32(traces)
}

pub fn decode_u64_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let traces = data.chunks_exact(8)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::BigEndian => u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::SwappedWord => u64::from_be_bytes([b[1], b[0], b[3], b[2], b[5], b[4], b[7], b[6]]),
        })
        .collect();

    TraceData::U64(traces)
}


pub fn decode_i16_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let traces = data.chunks_exact(2)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => i16::from_le_bytes([b[0], b[1]]),
            ByteOrder::BigEndian => i16::from_be_bytes([b[0], b[1]]),
            ByteOrder::SwappedWord => i16::from_be_bytes([b[1], b[0]]),
        })
        .collect();

    TraceData::I16(traces)
}

pub fn decode_i24_trace(data: &[u8], byte_order: ByteOrder) -> Result<TraceData, SegyError> {
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

pub fn decode_i32_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let traces = data.chunks_exact(4)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::SwappedWord => i32::from_be_bytes([b[1], b[0], b[3], b[2]]),
        })
        .collect();

    TraceData::I32(traces)
}

pub fn decode_i64_trace(data: &[u8], byte_order: ByteOrder) -> TraceData {
    let traces = data.chunks_exact(8)
        .map(|b| match byte_order {
            ByteOrder::LittleEndian => i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::BigEndian => i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            ByteOrder::SwappedWord => i64::from_be_bytes([b[1], b[0], b[3], b[2], b[5], b[4], b[7], b[6]]),
        })
        .collect();

    TraceData::I64(traces)
}