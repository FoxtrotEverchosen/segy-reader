"""`SegyFile.get_header`: encoding detection, extended headers, line layout, bad input."""

import pytest
from segy_factory import cards, make_segy

from fastsegy._fastsegy import SegyFile

MAIN = [f"C{i:2d} MAIN HEADER LINE {i}" for i in range(1, 41)]
EXT = [f"C{i:2d} EXTENDED HEADER LINE {i}" for i in range(1, 41)]


def expected(*blocks) -> str:
    """What get_header should return: 80-column lines joined with newlines."""
    return "\n".join(line.ljust(80) for block in blocks for line in block)


def header_of(tmp_path, **kwargs) -> str:
    return SegyFile(make_segy(tmp_path / "h.sgy", **kwargs)).get_header()


@pytest.mark.parametrize("ebcdic", [False, True], ids=["ascii", "ebcdic"])
def test_header_is_returned_as_forty_80_column_lines(tmp_path, ebcdic):
    text = header_of(tmp_path, text=cards(MAIN), ebcdic=ebcdic)

    lines = text.split("\n")
    assert len(lines) == 40
    assert all(len(line) == 80 for line in lines)
    assert text == expected(MAIN)


@pytest.mark.parametrize("ebcdic", [False, True], ids=["ascii", "ebcdic"])
def test_extended_headers_follow_the_main_header(tmp_path, ebcdic):
    text = header_of(tmp_path, text=cards(MAIN), ext_texts=[cards(EXT)], ebcdic=ebcdic)
    assert text == expected(MAIN, EXT)


def test_short_header_is_padded_to_full_size(tmp_path):
    text = header_of(tmp_path, text="C 1 ONLY ONE LINE")
    assert text.split("\n")[0] == "C 1 ONLY ONE LINE".ljust(80)
    assert len(text.split("\n")) == 40


def test_header_of_an_empty_trace_file_is_readable_in_every_byte_order(tmp_path):
    for order in ("big", "little", "swapped"):
        assert header_of(tmp_path, order=order, text=cards(MAIN)) == expected(MAIN)

def test_ascii_header_whose_last_byte_is_an_at_sign_is_not_taken_for_ebcdic(tmp_path):
    # '@' is 0x40, which is also the EBCDIC space the detection looks for.
    last = "C40 END TEXTUAL HEADER".ljust(79) + "@"
    lines = MAIN[:39] + [last]
    assert header_of(tmp_path, text=cards(lines)) == expected(MAIN[:39], [last])


def test_ebcdic_header_that_is_not_space_padded_is_still_detected(tmp_path):
    # The last card ends in '.' rather than a space, so the last byte is not 0x40.
    last = "C40 END TEXTUAL HEADER".ljust(79) + "."
    lines = MAIN[:39] + [last]
    assert header_of(tmp_path, text=cards(lines), ebcdic=True) == expected(MAIN[:39], [last])

