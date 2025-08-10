import csv
import numpy as np
import matplotlib.pyplot as plt
from typing import List, Tuple, Optional

# Path to your CSV file
csv_file = r"C:\Users\matth\Documents\GitHub\fractal-renderer\src\core\test_assets\mandelbrot_frame_rate_sweep.csv"

# ---------- Load CSV (two columns, no header) ----------
data: List[Tuple[float, float]] = []
with open(csv_file, newline="") as f:
    reader = csv.reader(f)
    for row in reader:
        if len(row) != 2:
            continue
        try:
            x = float(row[0])
            y = float(row[1])
            data.append((x, y))
        except ValueError:
            continue  # skip malformed rows

# ---------- Split into traces from 0 -> 1 (tolerant to FP error) ----------
traces: List[List[Tuple[float, float]]] = []
current_trace: List[Tuple[float, float]] = []
EPS = 1e-9

for x, y in data:
    current_trace.append((x, y))
    if abs(x - 1.0) < EPS:
        traces.append(current_trace)
        current_trace = []

if current_trace:
    traces.append(current_trace)

# ---------- Fitting: y = A * exp(-x) ----------
def fit_exp_fixed(xs: np.ndarray, ys: np.ndarray) -> Optional[float]:
    """
    Fit y = A * exp(-x) with B fixed at 1. Returns A or None if insufficient data.
    """
    mask = ys > 0.0
    if mask.sum() < 1:
        return None
    # y / exp(-x) = A
    A_vals = ys[mask] / np.exp(-xs[mask])
    return float(np.mean(A_vals))

# Precompute fits and labels
fit_params: List[Optional[float]] = []
legend_labels: List[str] = []

for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    xs_arr = np.asarray(xs, dtype=float)
    ys_arr = np.asarray(ys, dtype=float)
    A = fit_exp_fixed(xs_arr, ys_arr)
    fit_params.append(A)
    if A is None:
        label = f"Trace {i} (fit: insufficient data)"
    else:
        label = f"Trace {i} (A={A:.3g})"
    legend_labels.append(label)

# ---------- First plot: linear y ----------
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    line_plot, = plt.plot(xs, ys, marker='o', linestyle='-', label=legend_labels[i-1])

    # Plot fitted curve if available
    A = fit_params[i-1]
    if A is not None:
        x_smooth = np.linspace(0.0, 1.0, 200)
        y_smooth = A * np.exp(-x_smooth)
        plt.plot(x_smooth, y_smooth, linestyle='--', color=line_plot.get_color())

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data")
plt.title("Repeated Measurements vs Incrementer Value (Linear Y)")
plt.grid(True)
plt.legend()

# ---------- Second plot: log y ----------
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    line_plot, = plt.plot(xs, ys, marker='o', linestyle='-', label=legend_labels[i-1])

    # Plot fitted curve if available
    A = fit_params[i-1]
    if A is not None:
        x_smooth = np.linspace(0.0, 1.0, 200)
        y_smooth = A * np.exp(-x_smooth)
        plt.plot(x_smooth, y_smooth, linestyle='--', color=line_plot.get_color())

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data (log scale)")
plt.title("Repeated Measurements vs Incrementer Value (Log Y)")
plt.yscale("log")
plt.grid(True, which="both", linestyle="--", linewidth=0.5)
plt.legend()

# ---------- Show both ----------
plt.show()
