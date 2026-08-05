import numpy as np
from typing import Dict, Any, Optional


class SegyFile:
    def __init__(self, path: str) -> None: ...
    def get_trace(self, trace_number: int) -> np.ndarray: ...
    def get_trace_range(self, start: int, end: int) -> np.ndarray: ...
    def get_metadata(self) -> Dict[str, Any]: ...
    def get_header(self) -> str: ...


class BinaryHeaderConfig:
    def __init__(
            self,
            sample_interval: int,
            samples_per_trace: int,
            data_format: int,
            revision_number: int,
            fixed_length: int,
            byte_order: int,
            bytes_per_sample: int,
            ensemble_fold = None,
            trace_sorting_code = None,
            measurement_system = None) -> None: ...


def save_segy(file_path: str, textual_header: str, b_header_config: BinaryHeaderConfig, raw_traces: list[int], is_ascii: bool, n_traces: int) -> None:
    ...

