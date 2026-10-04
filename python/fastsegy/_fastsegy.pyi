
"""Type stubs for the Rust extension module ``_fastsegy`` (SEG-Y Rev 0 and Rev 1)."""

from typing import Any, Optional

import numpy as np
import numpy.typing as npt

class SegyFile:
    """A SEG-Y file opened for reading (memory-mapped).

    The file must not be modified or truncated while the object is alive.
    Trace numbers are 1-based.

    Element type of the returned arrays, by data sample format:

    ===========  =========  ===========================================
    Format       dtype      Notes
    ===========  =========  ===========================================
    IBM float    float32    code 1, converted to IEEE
    int32        int32      code 2
    int16        int16      code 3
    IEEE f32     float32    code 5
    IEEE f64     float64    code 6
    int24        int32      code 7, sign-extended
    int8         int8       code 8
    int64        int64      code 9
    uint32       uint32     code 10
    uint16       uint16     code 11
    uint64       uint64     code 12
    uint24       uint32     code 15
    uint8        uint8      code 16
    ===========  =========  ===========================================

    Format 4 (fixed point with gain) can be opened but not decoded.
    """

    def __init__(self, path: str) -> None:
        """Open ``path`` and index its traces.

        Raises:
            OSError: the file is missing, shorter than 3600 bytes, has an unrecognised byte-order
                field or data format, or is Revision 2 or later.
        """
        ...

    def get_trace(self, trace_number: int) -> npt.NDArray[Any]:
        """Return one trace as a 1-D array of its samples.

        Raises:
            ValueError: ``trace_number`` is 0 or larger than the number of traces, or the data
                format cannot be decoded.
        """
        ...

    def get_trace_range(self, start: int, end: int) -> npt.NDArray[Any]:
        """Return traces ``start`` to ``end`` as a 2-D array of shape ``(n_traces, n_samples)``.

        Both ends are 1-based and ``end`` is inclusive. ``start`` must be lower than ``end``, so
        a range holds at least two traces; use :meth:`get_trace` for a single one.

        Raises:
            ValueError: the range is invalid, the request is larger than the memory limit, or
                the data format cannot be decoded.
            TypeError: the traces have different lengths and cannot form a 2-D array.
        """
        ...

    def get_metadata(self) -> dict[str, Any]:
        """Summary of the file.

        Keys: ``"Trace Count"`` (int), ``"Samples Per Trace"`` (int), ``"Sample Interval"``
        (int, microseconds), ``"Bytes Per Sample"`` (int), ``"Data Format"`` (str),
        ``"Environment"``, ``"Dimensionality"`` and ``"Layout"`` (str), ``"Is time lapsed"``
        (bool), ``"Extended Text Header Count"`` (int), ``"Byte Order"`` (``"Big Endian"``,
        ``"Little Endian"`` or ``"Swapped Word"``) and ``"Revision Standard"`` (int, 0 or 1).
        """
        ...

    def get_header(self) -> str:
        """Textual header, with any extended textual headers appended.

        EBCDIC headers are converted to text. The result is split into lines of exactly 80
        characters joined by ``"\\n"`` (40 lines per header block).

        Raises:
            OSError: the file is too short for the extended headers its binary header declares.
        """
        ...

class BinaryHeaderConfig:
    """Binary header values for :func:`save_segy`.

    Attributes with the same names as the constructor arguments can be read and assigned.

    Args:
        sample_interval: microseconds between samples.
        samples_per_trace: samples in every trace (0 to 65535).
        data_format: SEG-Y data sample format code (for example 5 = IEEE f32, 6 = IEEE f64).
        revision_number: ``0x0100`` for Revision 1.0 (major revision in the high byte, minor in
            the low byte).
        fixed_length: fixed-length trace flag (1 = all traces have the same length, 0 = unknown).
        byte_order: ``0x01020304`` (big-endian), ``0x04030201`` (little-endian), ``0x02010403``
            (swapped word) or ``0`` (big-endian, as in pre-Rev 2 files).
        bytes_per_sample: size of one sample in ``raw_traces``.
        ensemble_fold: optional, 0 when omitted.
        measurement_system: optional, 0 when omitted.
        trace_sorting_code: optional, 0 when omitted.
    """

    sample_interval: int
    samples_per_trace: int
    data_format: int
    revision_number: int
    fixed_length: int
    byte_order: int
    bytes_per_sample: int
    ensemble_fold: Optional[int]
    measurement_system: Optional[int]
    trace_sorting_code: Optional[int]

    def __init__(
        self,
        sample_interval: int,
        samples_per_trace: int,
        data_format: int,
        revision_number: int,
        fixed_length: int,
        byte_order: int,
        bytes_per_sample: int,
        ensemble_fold: Optional[int] = None,
        measurement_system: Optional[int] = None,
        trace_sorting_code: Optional[int] = None,
    ) -> None: ...

def save_segy(
    file_path: str,
    textual_header: str,
    b_header_config: BinaryHeaderConfig,
    raw_traces: bytes,
    is_ascii: bool,
    n_traces: int,
) -> None:
    """Write a SEG-Y file, replacing ``file_path`` if it exists.

    Args:
        file_path: output path.
        textual_header: header text, 80 columns per card and no newlines (the main header holds
            3200 characters; the rest goes into extended textual headers).
        b_header_config: binary header values.
        raw_traces: the samples of all traces, one after another, **already encoded in the byte
            order named by** ``b_header_config.byte_order``. The length must be exactly
            ``n_traces * samples_per_trace * bytes_per_sample``.
        is_ascii: encode the textual headers as ASCII (``True``) or EBCDIC (``False``).
        n_traces: number of traces in ``raw_traces``.

    Trace headers are written empty except for the number of samples.

    Raises:
        ValueError: unknown byte-order value or ``raw_traces`` has the wrong length.
        TypeError: ``raw_traces`` is not a ``bytes`` object (a list of ints is not accepted).
        OSError: the file cannot be written.
    """
    ...

