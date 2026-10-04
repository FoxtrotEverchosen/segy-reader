"""`save_segy` -> `SegyFile` round trips, driven the way the GUI's `save_data()` drives them.

The data is encoded with the independent writer in `segy_factory`, so the GUI's own handling of
byte order is not tested here (see the note about swapped-word files in the summary).
"""

import numpy as np
import pytest
from segy_factory import INDICATOR_CONST, ORDER_NAMES, ORDERS, cards, encode, make_segy

from fastsegy._fastsegy import BinaryHeaderConfig, SegyFile, save_segy

MAIN = [f"C{i:2d} MAIN HEADER LINE {i}" for i in range(1, 41)]
EXT = [f"C{i:2d} EXTENDED HEADER LINE {i}" for i in range(1, 41)]
NAME_TO_ORDER = {name: order for order, name in ORDER_NAMES.items()}

VALUES = [[1.5, -2.25, 3.0, 4.125], [5.0, 6.5, -7.75, 8.0], [9.0, 10.0, 11.0, -12.5]]


def source_file(tmp_path, *, order="big", ebcdic=False, ext=False, name="src.sgy"):
    traces = [encode(t, "f4", order) for t in VALUES]
    return make_segy(
        tmp_path / name,
        order=order,
        traces=traces,
        ebcdic=ebcdic,
        text=cards(MAIN),
        ext_texts=[cards(EXT)] if ext else [],
    )


def save_like_gui(src: str, dst: str, start: int, end: int) -> None:
    """What `save_data()` does: re-save the displayed range as f64 with the header text the
    dialog returns (`get_header()` with the newlines removed)."""
    f = SegyFile(src)
    md = f.get_metadata()
    order = NAME_TO_ORDER[md["Byte Order"]]

    traces = f.get_trace_range(start, end)  # shape (n_traces, n_samples)
    n_traces, n_samples = traces.shape
    header = f.get_header().replace("\n", "")

    cfg = BinaryHeaderConfig(
        md["Sample Interval"],
        n_samples,
        6,  # IEEE f64, as in the GUI
        0x0100,  # Revision 1.0, as in the GUI
        0,
        INDICATOR_CONST[order],
        8,
    )
    raw = encode(traces.astype(np.float64), "f8", order)
    save_segy(dst, header, cfg, raw, True, n_traces)


@pytest.mark.parametrize("order", ORDERS)
def test_data_and_metadata_survive_a_save(tmp_path, order):
    src = source_file(tmp_path, order=order)
    dst = str(tmp_path / "out.sgy")
    save_like_gui(src, dst, 1, 3)

    out = SegyFile(dst)
    md = out.get_metadata()
    assert md["Trace Count"] == 3
    assert md["Samples Per Trace"] == 4
    assert md["Sample Interval"] == 2000
    assert md["Data Format"] == "IEEf64"
    assert md["Byte Order"] == ORDER_NAMES[order]

    assert out.get_trace_range(1, 3).dtype == np.float64
    np.testing.assert_array_equal(out.get_trace_range(1, 3), np.array(VALUES))
    np.testing.assert_array_equal(out.get_trace(2), VALUES[1])


@pytest.mark.parametrize("order", ORDERS)
def test_a_sub_range_can_be_saved(tmp_path, order):
    src = source_file(tmp_path, order=order)
    dst = str(tmp_path / "out.sgy")
    save_like_gui(src, dst, 2, 3)

    out = SegyFile(dst)
    assert out.get_metadata()["Trace Count"] == 2
    np.testing.assert_array_equal(out.get_trace_range(1, 2), np.array(VALUES[1:]))


@pytest.mark.parametrize("order", ORDERS)
def test_saved_file_reports_revision_1(tmp_path, order):
    # 0x0100 is Rev 1.0, whatever the byte order of the file.
    dst = str(tmp_path / "out.sgy")
    save_like_gui(source_file(tmp_path, order=order), dst, 1, 3)
    assert SegyFile(dst).get_metadata()["Revision Standard"] == 1


# ---------------------------------------------------------------------------------------------
# Textual header: open -> edit dialog -> save must not change or grow the header
# ---------------------------------------------------------------------------------------------

HEADER_CASES = [
    pytest.param(False, False, id="ascii"),
    pytest.param(True, False, id="ebcdic"),
    pytest.param(False, True, id="ascii+extended"),
    pytest.param(True, True, id="ebcdic+extended"),
]


@pytest.mark.parametrize(("ebcdic", "ext"), HEADER_CASES)
def test_header_text_survives_a_save(tmp_path, ebcdic, ext):
    src = source_file(tmp_path, ebcdic=ebcdic, ext=ext)
    dst = str(tmp_path / "out.sgy")
    save_like_gui(src, dst, 1, 3)

    before, after = SegyFile(src), SegyFile(dst)
    assert after.get_header() == before.get_header()
    ext_key = "Extended Text Header Count"
    assert after.get_metadata()[ext_key] == before.get_metadata()[ext_key]


@pytest.mark.parametrize(("ebcdic", "ext"), HEADER_CASES)
def test_repeated_saves_do_not_grow_the_file(tmp_path, ebcdic, ext):
    path = source_file(tmp_path, ebcdic=ebcdic, ext=ext)
    sizes, counts = [], []
    for cycle in range(3):
        dst = str(tmp_path / f"cycle{cycle}.sgy")
        save_like_gui(path, dst, 1, 3)
        path = dst
        sizes.append((tmp_path / f"cycle{cycle}.sgy").stat().st_size)
        counts.append(SegyFile(dst).get_metadata()["Extended Text Header Count"])

    assert len(set(sizes)) == 1, f"file size changed between saves: {sizes}"
    assert len(set(counts)) == 1, f"extended header count changed between saves: {counts}"
