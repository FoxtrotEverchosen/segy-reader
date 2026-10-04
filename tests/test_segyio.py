"""Cross-check against segyio, an independent SEG-Y reader.

Files come from the independent writer in `segy_factory`, so a value read the same wrong way by
this library and by the test fixtures would still disagree with segyio. segyio only reads big- and
little-endian files and formats 1, 2, 3, 5 and 8, which limits the matrix below.
"""

import numpy as np
import pytest
from segy_factory import encode, make_segy

from fastsegy._fastsegy import SegyFile

segyio = pytest.importorskip("segyio")

# (format code, dtype used to write the samples)
FORMATS = [(2, "i4"), (3, "i2"), (5, "f4"), (8, "i1")]
VALUES = [[1, -2, 3, -4, 5], [100, 99, -98, 97, -96], [0, 1, 0, -1, 0]]


def segyio_traces(path, order):
    with segyio.open(path, "r", ignore_geometry=True, endian=order) as f:
        # np.array copies: segyio reuses one buffer while iterating over traces
        traces = np.stack([np.array(t) for t in f.trace])
        return traces, f.samples.size, int(f.bin[segyio.BinField.Interval])


@pytest.mark.parametrize("order", ["big", "little"])
@pytest.mark.parametrize(("code", "dtype"), FORMATS)
def test_traces_match_segyio(tmp_path, order, code, dtype):
    traces = [encode(t, dtype, order) for t in VALUES]
    path = make_segy(tmp_path / "f.sgy", order=order, fmt=code, samples=5, traces=traces)

    expected, n_samples, interval = segyio_traces(path, order)
    f = SegyFile(path)

    assert f.get_metadata()["Samples Per Trace"] == n_samples
    assert f.get_metadata()["Sample Interval"] == interval
    np.testing.assert_array_equal(f.get_trace_range(1, 3).astype(np.float32), expected)


@pytest.mark.parametrize("order", ["big", "little"])
def test_ibm_float_matches_segyio(tmp_path, order):
    # 100.0, -118.625, 1.0, 0.5, 0.0 as IBM words
    words = ["42640000", "C276A000", "41100000", "40800000", "00000000"]
    raw = b"".join(bytes.fromhex(w) if order == "big" else bytes.fromhex(w)[::-1] for w in words)
    path = make_segy(tmp_path / "ibm.sgy", order=order, fmt=1, samples=5, traces=[raw, raw])

    expected, _, _ = segyio_traces(path, order)
    np.testing.assert_array_equal(SegyFile(path).get_trace_range(1, 2), expected)
    np.testing.assert_array_equal(expected[0], [100.0, -118.625, 1.0, 0.5, 0.0])
