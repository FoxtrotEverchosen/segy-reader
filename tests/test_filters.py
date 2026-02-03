import numpy as np
import pytest

from fastsegy.processing import running_average, median_xy_filter


def test_running_average_basic():
    data = np.array([[0., 0, 0, 10, 10, 0, 0, 0],
                     [1, 1, 1, 0, 0, 0, 1, 1],
                     [0, 0, 0, 0, 0, 0, 0, 0,]])
    params = {"range": 2, "start_time": 0}

    result = running_average(params, data, sample_interval=1000)

    expected = np.array([[0, 0, 10/3., 20/3., 20/3., 10/3., 0, 0],
                         [1, 1, 2/3., 1/3., 0, 1/3., 2/3., 1],
                         [0, 0, 0, 0, 0, 0, 0, 0,]])
    np.testing.assert_allclose(result, expected)


def test_running_average_invalid_window():
    try:
        data = np.array([0, 0, 10, 0, 0])
        params = {"range": 15000, "start_time": 0}
        running_average(params, data, sample_interval=1000)
        assert False, "Expected ValueError"
    except ValueError:
        assert True


def test_xy_median_identity_window():
    data = np.random.randn(10, 5)

    params = {
        "x": 1,
        "y": 1,
        "start_time": 0,
        "end_time": None
    }

    out = median_xy_filter(params, data, sample_interval=1000)

    assert np.allclose(out, data)


def test_xy_median_removes_spike():
    data = np.ones((10, 5))
    data[5, 2] = 1000  # spike

    params = {
        "x": 3,
        "y": 3,
        "start_time": 0,
        "end_time": None
    }

    out = median_xy_filter(params, data, sample_interval=1000)

    assert out[5, 2] == 1.0


def test_xy_median_invalid_window():
    data = np.ones((10, 5))

    params = {
        "x": 0,
        "y": 3,
        "start_time": 0,
        "end_time": None
    }

    with pytest.raises(ValueError):
        median_xy_filter(params, data, sample_interval=1000)


def test_xy_median_edges():
    data = np.random.randn(10, 5)

    params = {
        "x": 5,
        "y": 5,
        "start_time": 0,
        "end_time": None
    }

    out = median_xy_filter(params, data, sample_interval=1000)

    assert out.shape == data.shape
    assert not np.isnan(out).any()
