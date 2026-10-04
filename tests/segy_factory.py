"""Independent SEG-Y writer for the tests.

It uses no fastsegy code (not even `save_segy`): every offset comes from the SEG-Y spec tables and
text is encoded with Python's own `cp037` codec, so a mistake shared by the reader and the writer
cannot hide behind a passing round trip.
"""

import struct
from pathlib import Path

import numpy as np

ORDERS = ("big", "little", "swapped")
ORDER_NAMES = {"big": "Big Endian", "little": "Little Endian", "swapped": "Swapped Word"}
# The integer constants the GUI passes as `byte_order`
INDICATOR_CONST = {"big": 0x01020304, "little": 0x04030201, "swapped": 0x02010403}
_INDICATOR_BYTES = {"big": bytes([1, 2, 3, 4]), "little": bytes([4, 3, 2, 1]), "swapped": bytes([2, 1, 4, 3])}


def u16(value: int, order: str) -> bytes:
    """16-bit field. A swapped-word file is equivalent to little-endian for 2-byte values."""
    return struct.pack(">H" if order == "big" else "<H", value)


def encode(values, dtype: str, order: str) -> bytes:
    """Encode samples the way a file of the given byte order stores them."""
    dt = np.dtype(dtype)
    be = np.asarray(values, dtype=dt.newbyteorder(">")).tobytes()
    if dt.itemsize == 1 or order == "big":
        return be
    a = np.frombuffer(be, np.uint8).reshape(-1, dt.itemsize)
    if order == "little":
        a = a[:, ::-1]  # [A,B,C,D] -> [D,C,B,A]
    else:
        a = a.reshape(-1, dt.itemsize // 2, 2)[:, :, ::-1].reshape(-1, dt.itemsize)  # -> [B,A,D,C]
    return a.tobytes()


def cards(lines) -> str:
    """Pad each line to exactly 80 columns and concatenate, like real card images."""
    return "".join(line[:80].ljust(80) for line in lines)


def text_block(text: str, ebcdic: bool) -> bytes:
    raw = text.encode("cp037" if ebcdic else "latin-1")
    assert len(raw) <= 3200
    return raw + (b"\x40" if ebcdic else b"\x20") * (3200 - len(raw))


def make_segy(
    path,
    *,
    order="big",
    fmt=5,
    samples=4,
    interval=2000,
    traces=(),
    header_samples=None,
    ebcdic=False,
    text="",
    ext_texts=(),
    rev=1,
    zero_indicator=False,
    ext_count_field=None,
) -> str:
    """Write a SEG-Y file. `traces` are raw sample bytes, already encoded for `order`.

    `header_samples` (one value per trace) goes into trace header bytes 115-116; by default every
    trace header repeats `samples`. `ext_count_field` writes a count that may not match reality.
    """
    h = bytearray(400)

    def put(at: int, value: int) -> None:
        h[at : at + 2] = u16(value, order)

    put(16, interval)  # 3217-3218
    put(20, samples)  # 3221-3222
    put(24, fmt)  # 3225-3226
    put(304, len(ext_texts) if ext_count_field is None else ext_count_field)  # 3505-3506
    h[300] = rev  # 3501 major revision (a single byte)
    if not zero_indicator:
        h[96:100] = _INDICATOR_BYTES[order]  # 3297-3300

    out = bytearray(text_block(text, ebcdic)) + h
    for ext in ext_texts:
        out += text_block(ext, ebcdic)
    for i, data in enumerate(traces):
        n = samples if header_samples is None else header_samples[i]
        header = bytearray(240)
        header[114:116] = u16(n, order)  # trace header bytes 115-116
        out += header + data

    Path(path).write_bytes(bytes(out))
    return str(path)
