import numpy as np
import math
from scipy.ndimage import median_filter


def profile_flip(params: dict, data: np.ndarray):
    axis = params["axis"]

    if axis == "x":
        return np.fliplr(data)
    else:
        return np.flipud(data)


def running_average(params: dict, data: np.ndarray, sample_interval):
    """
    Horizontal running average filter across traces.

    params: dict (average_traces, start_sample, end_sample)
    data: np.ndarray [traces, samples]
    """

    average_traces = int(params.get('range'))
    if average_traces <= 0 or average_traces > 1024:
        raise ValueError("average_traces must be in range 1..1024")

    samples, traces = data.shape

    start_time = params.get("start_time")
    end_time = params.get("end_time")

    if start_time is None or start_time == "":
        start_sample = 0
    else:
        start_sample = time_to_sample_index(
            float(start_time),
            sample_interval,
        )

    if end_time is None or end_time == "":
        end_sample = samples - 1
    else:
        end_sample = time_to_sample_index(
            float(end_time),
            sample_interval,
        )

    if start_sample < 0 or end_sample >= samples * sample_interval:
        raise ValueError("Sample range out of bounds")

    output = data.copy()

    for t in range(start_sample, end_sample+1):
        row = data[t, :]
        output[t, :] = horizontal_running_mean(row, average_traces)

    return output


def horizontal_running_mean(row, window):
    half = window // 2
    csum = np.cumsum(np.insert(row, 0, 0))
    out = np.zeros_like(row, dtype=float)

    for i in range(len(row)):
        left = max(0, i - half)
        right = min(len(row), i + half + 1)
        out[i] = (csum[right] - csum[left]) / (right - left)

    return out


def median_xy_filter(params: dict, data: np.ndarray, sample_interval):
    """
    Filters data by median.

    params: dict (x, y, start_time, end_time) x, y - size of the filter window, start_time, end_time - range of filtered samples
    data: np.ndarray [traces, samples]
    """

    x_window = int(params["x"])
    y_window = int(params["y"])

    if x_window <= 0 or x_window > 256:
        raise ValueError("x must be in range 1..256")

    if y_window <= 0:
        raise ValueError("y must be > 0")

    samples, traces = data.shape
    start_time = params.get("start_time")
    end_time = params.get("end_time")

    if start_time is None or start_time == "":
        start = 0
    else:
        start = time_to_sample_index(
            float(start_time),
            sample_interval,
        )

    if end_time is None or end_time == "":
        end = samples - 1
    else:
        end = time_to_sample_index(
            float(end_time),
            sample_interval,
        )

    out = data.copy()

    filtered = median_filter(
        data,
        size=(y_window, x_window),
        mode="nearest"
    )

    if end is None:
        out[start:] = filtered[start:]
    else:
        out[start:end+1] = filtered[start:end+1]

    return out


def time_to_sample_index(time_sec, sample_interval_us):
    """
    Converts time in seconds to the first sample index at or after that time.

    time_sec: float
    sample_interval_us: float
    """
    dt = sample_interval_us / 1_000_000.0
    idx = math.ceil(time_sec / dt)

    return idx
