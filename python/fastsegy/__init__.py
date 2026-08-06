"""Fast SEG-Y file parser with Rust backend."""
__version__ = "0.1.0"

from ._fastsegy import SegyFile, BinaryHeaderConfig, save_segy

__all__ = ["SegyFile", "BinaryHeaderConfig", "save_segy"]
