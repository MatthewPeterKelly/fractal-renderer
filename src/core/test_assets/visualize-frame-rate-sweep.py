import csv
import math
from typing import List, Tuple, Optional

import numpy as np
import matplotlib.pyplot as plt

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

# ---------- Fitting: y = A * exp(-B * x) via log-linear least squares ----------
def fit_exp_model(xs: np.ndarray, ys: np.ndarray) -> Optional[Tuple[float, float]]:
    """
    Fit y = A * exp(-B * x). Returns (A, B) or None if not enough valid data.
    Uses log-linear regression on y>0.
    """
    mask = ys > 0.0
    if mask.sum() < 2:
        return None
    x_fit = xs[mask]
    y_fit = ys[mask]
    ln_y = np.log(y_fit)
    # ln(y) = ln(A) - B*x -> linear fit: ln(y) = c + m*x
    m, c = np.polyfit(x_fit, ln_y, 1)
    A = float(np.exp(c))
    B = float(-m)
    return (A, B)

# Precompute fits and labels
fit_params: List[Optional[Tuple[float, float]]] = []
legend_labels: List[str] = []

for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    xs_arr = np.asarray(xs, dtype=float)
    ys_arr = np.asarray(ys, dtype=float)
    params = fit_exp_model(xs_arr, ys_arr)
    fit_params.append(params)
    if params is None:
        label = f"Trace {i} (fit: insufficient data)"
    else:
        A, B = params
        label = f"Trace {i} (A={A:.3g}, B={B:.3g})"
    legend_labels.append(label)

# ---------- First plot: linear y ----------
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    line_plot, = plt.plot(xs, ys, marker='o', linestyle='-', label=legend_labels[i-1])

    # Plot fitted curve if available
    params = fit_params[i-1]
    if params is not None:
        A, B = params
        x_smooth = np.linspace(0.0, 1.0, 200)
        y_smooth = A * np.exp(-B * x_smooth)
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
    params = fit_params[i-1]
    if params is not None:
        A, B = params
        x_smooth = np.linspace(0.0, 1.0, 200)
        y_smooth = A * np.exp(-B * x_smooth)
        plt.plot(x_smooth, y_smooth, linestyle='--', color=line_plot.get_color())

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data (log scale)")
plt.title("Repeated Measurements vs Incrementer Value (Log Y)")
plt.yscale("log")
plt.grid(True, which="both", linestyle="--", linewidth=0.5)
plt.legend()

# ---------- Show both ----------
plt.show()
