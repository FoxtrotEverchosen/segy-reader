use crate::types::{ByteOrder, DataFormat, DimensionalityType, EnvironmentType, LayoutType, SegyError};

#[derive(Debug)]
pub struct BinaryHeader {
    pub sample_interval: i16,
    pub samples_per_trace: usize,
    pub bytes_per_sample: usize,
    pub extended_text_header_count: usize,
    pub first_trace_offset: u64,
    pub data_trailer_count: i16,
    pub is_time_lapsed: bool,
    pub data_format: DataFormat,
    pub byte_order: ByteOrder,
    pub environment_type: EnvironmentType,
    pub dimensionality_type: DimensionalityType,
    pub layout_type: LayoutType,
    pub rev_version: u8,
}

pub fn parse_binary_header(buf: &[u8]) -> Result<BinaryHeader, SegyError> {
    const ENVIRONMENT_MASK: i16 = 0x07;
    const DIMENSIONALITY_MASK: i16 = 0x20;
    const LAYOUT_MASK: i16 = 0x780;

    let byte_order = &buf[96..100];
    let byte_order: ByteOrder = match byte_order {
        // Older SEG-Y files, following Revision 0 standard encode numbers BigEndian only, therefore
        // those files would most likely have this four bytes filled with 0
        [0x01, 0x02, 0x03, 0x04] | [0x00, 0x00, 0x00, 0x00] => ByteOrder::BigEndian,
        [0x04, 0x03, 0x02, 0x01] => ByteOrder::LittleEndian,
        [0x02, 0x01, 0x04, 0x03] => ByteOrder::SwappedWord,
        _ => {
            return Err(SegyError::UnsupportedDataFormat);
        }
    };

    let reader = HeaderReader::new(buf, 3200, byte_order);
    let rev_version = reader.read_u8(3501);

    // The current implementation could possibly not throw an error but also not parse a Rev >= 2.0 file correctly depending on file structure.
    // To avoid problems with displaying erroneously read data, the program will return error on header parsing instead.
    // Rev 2.0 was introduced in 2017 and as of now, there is still a very limited amount of tools that save seismic data in that standard
    if rev_version > 1 {
        return Err(SegyError::UnsupportedDataFormat);
    }

    let sample_interval = reader.read_i16(3217);
    let data_format = reader.read_i16(3225);
    let samples_per_trace = usize::try_from(reader.read_i16(3221)).expect("Samples per trace should never be negative");

    // As per the SEG-Y documentation this field could store -1 to represent variable number of ext. headers.
    // For now, the variable number of ext. textual headers will throw error
    let extended_text_header_count =
        usize::try_from(reader.read_i16(3505)).map_err(|_| SegyError::UnsupportedDataFormat)?;

    let bytes_per_sample: usize = match data_format {
        8 | 16 => 1,
        3 | 11 => 2,
        7 | 15 => 3,
        1 | 2 | 4 | 5 | 10 => 4,
        6 | 9 | 12 => 8,
        _ => return Err(SegyError::UnsupportedDataFormat),
    };

    // Can be zero -> guaranteed(?) no data trailers
    let data_trailer_count = reader.read_i16(3529);
    let first_trace_offset = reader.read_u64(3521);
    let fixed_length = reader.read_i16(3503);

    let data_format = match data_format {
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
        _ => return Err(SegyError::UnsupportedDataFormat),
    };

    let survey_type = reader.read_i16(3511);
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
        1152 => LayoutType::OceanBottomSensors,
        1280 => LayoutType::PseudoRandomSensor,
        _ => LayoutType::Unspecified,
    };

    Ok(BinaryHeader {
        sample_interval,
        samples_per_trace,
        bytes_per_sample,
        extended_text_header_count,
        first_trace_offset,
        data_trailer_count,
        is_time_lapsed,
        data_format,
        byte_order,
        environment_type,
        dimensionality_type,
        layout_type,
        rev_version,
    })
}

pub struct HeaderReader<'a> {
    buf: &'a [u8],
    base: usize,
    order: ByteOrder,
}

impl<'a> HeaderReader<'a> {
    pub(crate) fn new(buf: &'a [u8], base: usize, order: ByteOrder) -> Self {
        Self { buf, base, order }
    }

    fn offset(&self, doc_byte: usize) -> usize {
        // SEG-Y documentation presents parameters of headers as list of bytes-meaning pairs. Bytes
        // presented in docs are 1-based. Self.base represents number of byte that starts the header.
        // In case of binary header (assuming no record tape) that value would be 3200.
        // In that case 3201-st (1-based) byte starts the header.
        doc_byte - self.base - 1
    }

    fn read_u8(&self, doc_byte: usize) -> u8 {
        let ofs = self.offset(doc_byte);
        self.buf[ofs]
    }

    pub(crate) fn read_i16(&self, doc_byte: usize) -> i16 {
        let ofs = self.offset(doc_byte);
        let bytes = [self.buf[ofs], self.buf[ofs + 1]];
        match self.order {
            ByteOrder::BigEndian => i16::from_be_bytes(bytes),
            ByteOrder::LittleEndian => i16::from_le_bytes(bytes),
            ByteOrder::SwappedWord => i16::from_be_bytes([bytes[1], bytes[0]]),
        }
    }

    fn read_u64(&self, doc_byte: usize) -> u64 {
        let ofs = self.offset(doc_byte);
        let bytes = self.buf[ofs..ofs + 8]
            .try_into()
            .expect("Conversion to 8 element array should never fail");
        match self.order {
            ByteOrder::BigEndian => u64::from_be_bytes(bytes),
            ByteOrder::LittleEndian => u64::from_le_bytes(bytes),
            ByteOrder::SwappedWord => u64::from_be_bytes([
                bytes[1], bytes[0], bytes[3], bytes[2], bytes[5], bytes[4], bytes[7], bytes[6],
            ]),
        }
    }
}
