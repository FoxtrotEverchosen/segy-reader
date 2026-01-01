"""Fast SEG-Y file parser with Rust backend."""

# Import Rust functions
from ._fastsegy import (
    get_header,
    get_metadata,
    get_trace,
)

__version__ = "0.1.0"

__all__ = [
    "get_header",
    "get_metadata",
    "get_trace",
]