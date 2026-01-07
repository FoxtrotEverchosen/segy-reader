import fastsegy
import segyio
import numpy as np

path = r"C:/fastsegy/Kerry3D.segy"
seg_file = fastsegy.SegyFile(path)
nth_trace = 3500
data = seg_file.get_trace(nth_trace)

with np.printoptions(threshold=100):
    print(data)
print(f"Shape fastsegy: {data.shape}")
print(f"Data type: {data.dtype}")

with segyio.open(path, 'r', ignore_geometry=True) as f:

    trace = f.trace[nth_trace-1]   # segyio is 0-based

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
