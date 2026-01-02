import fastsegy
import segyio
import numpy as np

path = r"C:/fastsegy/Kerry3D.segy"
header = fastsegy.get_header(path)
print(header)

bin_header = fastsegy.get_metadata(path)
data = fastsegy.get_trace(path, 6500, bin_header["index"])   # fastsegy is 1-based

with np.printoptions(threshold=100):
    print(data)
print(f"Shape fastsegy: {data.shape}")
print(f"Data type: {data.dtype}")

with segyio.open(path, 'r', ignore_geometry=True) as f:

    trace = f.trace[6499]   # segyio is 0-based

    with np.printoptions(threshold=100):
        print(trace)

    print(f"Shape: {trace.shape}")
    print(f"Data type: {trace.dtype}")

# Shapes should never be different
assert data.shape == trace.shape

mse = np.mean((data - trace)**2)
difference = data - trace
avg_difference = np.mean(difference)
max_difference = np.max(difference)
min_difference = np.min(difference)

print(f"Max value from fastsegy: {np.max(data)}, Min value from fastsegy: {np.min(data)}")
print(f"Max value from segyio: {np.max(trace)}, Min value from segyio: {np.min(trace)}")
print(f"Difference: {avg_difference}, max: {max_difference}, min: {min_difference}, mse: {mse}")
print(type(header))