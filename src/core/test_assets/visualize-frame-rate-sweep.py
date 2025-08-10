import csv
import random
from typing import List, Tuple, Optional

import numpy as np
import matplotlib.pyplot as plt

# ----------------- CONFIG -----------------
csv_file = r"C:\Users\matth\Documents\GitHub\fractal-renderer\src\core\test_assets\mandelbrot_frame_rate_sweep.csv"
USE_TWO_PARAM_MODEL = False   # True => y = A*exp(-B*x), False => y = A*exp(-x)
MAX_TRACES_TO_PLOT = 10
RANDOM_SEED = 42            # set to None for non-deterministic selection
EPS = 1e-9                  # tolerance for detecting x == 1.0
# ------------------------------------------

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
for x, y in data:
    current_trace.append((x, y))
    if abs(x - 1.0) < EPS:
        traces.append(current_trace)
        current_trace = []
if current_trace:
    traces.append(current_trace)

# ---------- Fitting models ----------
def fit_two_param(xs: np.ndarray, ys: np.ndarray) -> Optional[Tuple[float, float]]:
    """
    Fit y = A * exp(-B * x) via log-linear least squares on y>0.
    Returns (A, B) or None if insufficient data.
    """
    mask = ys > 0.0
    if mask.sum() < 2:
        return None
    x_fit = xs[mask]
    ln_y = np.log(ys[mask])
    # ln(y) = c + m*x  with A = exp(c), B = -m
    m, c = np.polyfit(x_fit, ln_y, 1)
    A = float(np.exp(c))
    B = float(-m)
    return (A, B)

def fit_one_param(xs: np.ndarray, ys: np.ndarray) -> Optional[float]:
    """
    Fit y = A * exp(-x) with B fixed at 1.
    Using log form: ln(y) = ln(A) - x => ln(A) = mean(ln(y) + x) over y>0.
    Returns A or None if insufficient data.
    """
    mask = ys > 0.0
    if mask.sum() < 1:
        return None
    val = np.mean(np.log(ys[mask]) + xs[mask])
    A = float(np.exp(val))
    return A

# ---------- Randomly select up to MAX_TRACES_TO_PLOT ----------
if len(traces) > MAX_TRACES_TO_PLOT:
    if RANDOM_SEED is not None:
        random.seed(RANDOM_SEED)
    traces = random.sample(traces, MAX_TRACES_TO_PLOT)

# ---------- Compute fits and labels ----------
fit_params: List[Optional[Tuple[float, float] or float]] = []
legend_labels: List[str] = []

for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    xs_arr = np.asarray(xs, dtype=float)
    ys_arr = np.asarray(ys, dtype=float)

    if USE_TWO_PARAM_MODEL:
        params = fit_two_param(xs_arr, ys_arr)
        fit_params.append(params)
        if params is None:
            label = f"Trace {i} (fit: insufficient)"
        else:
            A, B = params
            label = f"Trace {i} (A={A:.3g}, B={B:.3g})"
    else:
        A = fit_one_param(xs_arr, ys_arr)
        fit_params.append(A)
        if A is None:
            label = f"Trace {i} (fit: insufficient)"
        else:
            label = f"Trace {i} (A={A:.3g})"

    legend_labels.append(label)

# ---------- Plot: linear y ----------
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    line_plot, = plt.plot(xs, ys, marker='o', linestyle='-', label=legend_labels[i-1])

    # Fitted curve overlay
    x_smooth = np.linspace(0.0, 1.0, 200)
    params = fit_params[i-1]
    if params is not None:
        if USE_TWO_PARAM_MODEL:
            A, B = params  # type: ignore
            y_smooth = A * np.exp(-B * x_smooth)
        else:
            A = params      # type: ignore
            y_smooth = A * np.exp(-x_smooth)
        plt.plot(x_smooth, y_smooth, linestyle='--', color=line_plot.get_color())

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data")
plt.title("Repeated Measurements vs Incrementer Value (Linear Y)")
plt.grid(True)
plt.legend()

# ---------- Plot: log y ----------
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    line_plot, = plt.plot(xs, ys, marker='o', linestyle='-', label=legend_labels[i-1])

    # Fitted curve overlay
    x_smooth = np.linspace(0.0, 1.0, 200)
    params = fit_params[i-1]
    if params is not None:
        if USE_TWO_PARAM_MODEL:
            A, B = params  # type: ignore
            y_smooth = A * np.exp(-B * x_smooth)
        else:
            A = params      # type: ignore
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
