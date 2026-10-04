"""Public API contract of the extension: dtypes, shapes, exception types, metadata keys."""

import numpy as np
import pytest
from segy_factory import INDICATOR_CONST, ORDER_NAMES, ORDERS, encode, make_segy

from fastsegy._fastsegy import BinaryHeaderConfig, SegyFile, save_segy

# (format code, dtype the library should return, numpy dtype used to write the samples)
FORMATS = [
    (2, np.int32, "i4"),
    (3, np.int16, "i2"),
    (5, np.float32, "f4"),
    (6, np.float64, "f8"),
    (8, np.int8, "i1"),
    (9, np.int64, "i8"),
    (10, np.uint32, "u4"),
    (11, np.uint16, "u2"),
    (12, np.uint64, "u8"),
    (16, np.uint8, "u1"),
]

THREE = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]]


def three_traces(tmp_path, *, order="big", fmt=5, dtype="f4"):
    traces = [encode(t, dtype, order) for t in THREE]
    return make_segy(tmp_path / "f.sgy", order=order, fmt=fmt, traces=traces)


# ---------------------------------------------------------------------------------------------
# Element types and shapes
# ---------------------------------------------------------------------------------------------


@pytest.mark.parametrize("order", ORDERS)
@pytest.mark.parametrize(("code", "np_dtype", "write_dtype"), FORMATS)
def test_dtype_values_and_shape_per_format(tmp_path, order, code, np_dtype, write_dtype):
    f = SegyFile(three_traces(tmp_path, order=order, fmt=code, dtype=write_dtype))

    one = f.get_trace(2)
    assert one.dtype == np_dtype
    assert one.shape == (4,)
    np.testing.assert_array_equal(one, THREE[1])

    block = f.get_trace_range(1, 3)
    assert block.dtype == np_dtype
    assert block.shape == (3, 4)
    np.testing.assert_array_equal(block, THREE)


@pytest.mark.parametrize("order", ["big", "little"])
def test_ibm_float_is_returned_as_float32(tmp_path, order):
    # IBM words 0x42640000 (100.0) and 0xC276A000 (-118.625), written for the given order
    words = [bytes.fromhex("42640000"), bytes.fromhex("C276A000")]
    raw = b"".join(w if order == "big" else w[::-1] for w in words)
    f = SegyFile(make_segy(tmp_path / "ibm.sgy", order=order, fmt=1, samples=2, traces=[raw]))

    trace = f.get_trace(1)
    assert trace.dtype == np.float32
    np.testing.assert_array_equal(trace, [100.0, -118.625])


@pytest.mark.parametrize("order", ["big", "little"])
def test_24_bit_formats_widen_to_32_bit_dtypes(tmp_path, order):
    def samples(values):  # 3-byte big-endian items, reversed per item for little-endian files
        items = [bytes(v) for v in values]
        return b"".join(i if order == "big" else i[::-1] for i in items)

    raw = samples([(0xFF, 0xFF, 0xFF), (0x01, 0x02, 0x03)])

    signed = SegyFile(make_segy(tmp_path / "i24.sgy", order=order, fmt=7, samples=2, traces=[raw]))
    assert signed.get_trace(1).dtype == np.int32
    np.testing.assert_array_equal(signed.get_trace(1), [-1, 66051])

    unsigned = SegyFile(make_segy(tmp_path / "u24.sgy", order=order, fmt=15, samples=2, traces=[raw]))
    assert unsigned.get_trace(1).dtype == np.uint32
    np.testing.assert_array_equal(unsigned.get_trace(1), [16777215, 66051])


def test_24_bit_swapped_word_is_reported_as_an_error(tmp_path):
    raw = bytes([1, 2, 3, 4, 5, 6])
    f = SegyFile(make_segy(tmp_path / "s.sgy", order="swapped", fmt=7, samples=2, traces=[raw]))
    with pytest.raises(ValueError):
        f.get_trace(1)


def test_obsolete_fixed_point_format_is_reported_as_an_error(tmp_path):
    f = SegyFile(make_segy(tmp_path / "fp.sgy", fmt=4, samples=2, traces=[bytes(8)]))
    with pytest.raises(ValueError):
        f.get_trace(1)


def test_ragged_range_fails_cleanly_but_single_traces_work(tmp_path):
    traces = [encode([1, 2], "f4", "big"), encode([3, 4, 5], "f4", "big")]
    f = SegyFile(make_segy(tmp_path / "r.sgy", traces=traces, header_samples=[2, 3]))

    assert f.get_trace(1).shape == (2,)
    assert f.get_trace(2).shape == (3,)
    # A 2-D array needs equal lengths. Currently reported as TypeError, ValueError would fit better.
    with pytest.raises((TypeError, ValueError)):
        f.get_trace_range(1, 2)


# ---------------------------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------------------------


@pytest.mark.parametrize("number", [0, 4, 1000])
def test_invalid_trace_numbers_raise_value_error(tmp_path, number):
    f = SegyFile(three_traces(tmp_path))
    with pytest.raises(ValueError):
        f.get_trace(number)


@pytest.mark.parametrize("start_end", [(3, 2), (0, 3), (1, 4), (5, 9)])
def test_invalid_ranges_raise_value_error(tmp_path, start_end):
    f = SegyFile(three_traces(tmp_path))
    with pytest.raises(ValueError):
        f.get_trace_range(*start_end)


def test_range_end_is_inclusive_and_one_based(tmp_path):
    f = SegyFile(three_traces(tmp_path))
    np.testing.assert_array_equal(f.get_trace_range(2, 3), THREE[1:])


def test_missing_file_raises_os_error(tmp_path):
    with pytest.raises(OSError):
        SegyFile(str(tmp_path / "missing.sgy"))


def test_unusable_files_raise_os_error(tmp_path):
    short = tmp_path / "short.sgy"
    short.write_bytes(bytes(3599))
    with pytest.raises(OSError):
        SegyFile(str(short))

    empty = tmp_path / "empty.sgy"
    empty.write_bytes(b"")
    with pytest.raises(OSError):
        SegyFile(str(empty))

    bad_order = bytearray(open(three_traces(tmp_path), "rb").read())
    bad_order[3296:3300] = b"\xff\xff\xff\xff"
    (tmp_path / "bad.sgy").write_bytes(bytes(bad_order))
    with pytest.raises(OSError):
        SegyFile(str(tmp_path / "bad.sgy"))

    with pytest.raises(OSError):  # revision 2 is not supported
        SegyFile(make_segy(tmp_path / "rev2.sgy", rev=2))


# ---------------------------------------------------------------------------------------------
# Metadata (the GUI relies on these keys and on the exact byte-order strings)
# ---------------------------------------------------------------------------------------------

EXPECTED_KEYS = {
    "Trace Count",
    "Samples Per Trace",
    "Sample Interval",
    "Bytes Per Sample",
    "Data Format",
    "Environment",
    "Dimensionality",
    "Is time lapsed",
    "Layout",
    "Extended Text Header Count",
    "Byte Order",
    "Revision Standard",
}


@pytest.mark.parametrize("order", ORDERS)
def test_metadata_keys_and_values(tmp_path, order):
    md = SegyFile(three_traces(tmp_path, order=order)).get_metadata()

    assert set(md) == EXPECTED_KEYS
    assert md["Trace Count"] == 3
    assert md["Samples Per Trace"] == 4
    assert md["Sample Interval"] == 2000
    assert md["Bytes Per Sample"] == 4
    assert md["Data Format"] == "IEEf32"
    assert md["Byte Order"] == ORDER_NAMES[order]
    assert md["Extended Text Header Count"] == 0
    assert md["Revision Standard"] == 1
    assert md["Is time lapsed"] is False


# ---------------------------------------------------------------------------------------------
# BinaryHeaderConfig and save_segy argument handling
# ---------------------------------------------------------------------------------------------


def test_config_getters_and_setters():
    cfg = BinaryHeaderConfig(2000, 4, 5, 0x0100, 0, INDICATOR_CONST["big"], 4)
    assert (cfg.sample_interval, cfg.samples_per_trace, cfg.data_format) == (2000, 4, 5)
    assert cfg.ensemble_fold is None
    cfg.sample_interval = 4000
    assert cfg.sample_interval == 4000


def test_optional_config_fields_keep_the_documented_positional_order():
    # The Rust signature is (..., ensemble_fold, measurement_system, trace_sorting_code).
    # The old .pyi listed trace_sorting_code before measurement_system.
    cfg = BinaryHeaderConfig(2000, 4, 5, 0x0100, 0, 0x01020304, 4, 24, 1, 4)
    assert (cfg.ensemble_fold, cfg.measurement_system, cfg.trace_sorting_code) == (24, 1, 4)


def test_config_accepts_sample_counts_above_i16_max():
    # Sample counts are unsigned 16-bit; the GUI passes the count read from the file.
    cfg = BinaryHeaderConfig(2000, 40_000, 5, 0x0100, 0, INDICATOR_CONST["big"], 4)
    assert cfg.samples_per_trace == 40_000


def test_save_segy_wants_bytes_not_a_list(tmp_path):
    cfg = BinaryHeaderConfig(2000, 1, 5, 0x0100, 0, INDICATOR_CONST["big"], 4)
    out = str(tmp_path / "o.sgy")
    with pytest.raises(TypeError):
        save_segy(out, "", cfg, [0, 0, 0, 0], True, 1)
    save_segy(out, "", cfg, bytes(4), True, 1)
    assert (tmp_path / "o.sgy").stat().st_size == 3600 + 240 + 4


def test_save_segy_rejects_an_unknown_byte_order(tmp_path):
    cfg = BinaryHeaderConfig(2000, 1, 5, 0x0100, 0, 0x12345678, 4)
    with pytest.raises(ValueError):
        save_segy(str(tmp_path / "o.sgy"), "", cfg, bytes(4), True, 1)
