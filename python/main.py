import fastsegy

header = fastsegy.get_header()
print(header)

bin_header = fastsegy.get_metadata()

import segyio
import numpy as np

# TODO: Add tests to check if results from different type conversions implemented in Rust are the same as in segyio

# Open the file
with segyio.open(r"C:\fastsegy\rust\Kerry3D.segy", 'r', ignore_geometry=True) as f:
    # Get the 350th trace (0-indexed, so index 349)
    trace_350 = f.trace[5000]

    # trace_350 is now a numpy array with the sample values
    with np.printoptions(threshold=5000):
        print(trace_350)

    print(f"Shape: {trace_350.shape}")
    print(f"Data type: {trace_350.dtype}")