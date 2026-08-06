use std::fmt::Display;

#[derive(Debug)]
pub enum DataFormat {
    IBMf32,          // Code: 1      bytes: 4
    I32,             // 2            4
    I16,             // 3            2
    FixedPointWGain, // 4            4           (Obsolete)
    IEEf32,          // 5            4
    IEEf64,          // 6            8
    I24,             // 7            3
    I8,              // 8            1
    I64,             // 9            8
    U32,             // 10           4
    U16,             // 11           2
    U64,             // 12           8
    U24,             // 15           3
    U8,              // 16           1
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
pub enum ByteOrder {
    BigEndian,
    LittleEndian,
    SwappedWord,
}

impl ByteOrder {
    pub(crate) fn as_str(self) -> &'static str {
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
    InvalidTraceRange { start: u32, end: u32, trace_count: usize },
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
            SegyError::TraceOutOfRange { requested, trace_count } => {
                format!("Trace out of range. Requested {requested} trace, out of {trace_count} traces")
            }
            SegyError::InvalidTraceRange {
                start,
                end,
                trace_count,
            } => {
                format!("Invalid trace range. ({start} to {end} in file with {trace_count} traces)")
            }
            SegyError::UnsupportedDataFormat => String::from("Unsupported data format"),
            SegyError::CorruptTrace => String::from("Corrupt trace segment"),
            SegyError::ParseFailure => String::from("Failed to parse data"),
            SegyError::DecodingError(e) => format!("Decoding error: {e}"),
            SegyError::RequestMemoryError => {
                String::from("Requested data exceeds your memory limit, try with smaller trace range.")
            }
            SegyError::InvalidArgument(e) => format!("Invalid argument: {e}"),
        };
        write!(f, "{result}")
    }
}

impl SegyError {
    pub(crate) fn kind(&self) -> &str {
        match self {
            SegyError::Io(_) => "IO",
            SegyError::TraceOutOfRange { .. } => "out_of_range",
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
pub enum EnvironmentType {
    Land,
    Marine,
    Transition,
    Downhole,
    Unspecified,
}

impl EnvironmentType {
    pub fn as_str(&self) -> &str {
        match self {
            EnvironmentType::Land => "Land",
            EnvironmentType::Marine => "Marine",
            EnvironmentType::Transition => "Transition",
            EnvironmentType::Downhole => "Downhole",
            EnvironmentType::Unspecified => "Unspecified",
        }
    }
}

#[derive(Debug)]
pub enum DimensionalityType {
    D1,
    D2,
    D3,
    Unspecified,
}

impl DimensionalityType {
    pub fn as_str(&self) -> &str {
        match self {
            DimensionalityType::D1 => "1D",
            DimensionalityType::D2 => "2D",
            DimensionalityType::D3 => "3D",
            DimensionalityType::Unspecified => "Unspecified",
        }
    }
}

#[derive(Debug)]
pub enum LayoutType {
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
        match self {
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

pub enum TraceData {
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
