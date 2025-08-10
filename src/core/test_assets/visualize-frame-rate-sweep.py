import csv
import matplotlib.pyplot as plt

# Path to your CSV file
csv_file = r"C:\Users\matth\Documents\GitHub\fractal-renderer\src\core\test_assets\mandelbrot_frame_rate_sweep.csv"

# Read CSV data
data = []
with open(csv_file, newline="") as f:
    reader = csv.reader(f)
    for row in reader:
        if len(row) != 2:
            continue  # skip malformed rows
        try:
            x = float(row[0])
            y = float(row[1])
            data.append((x, y))
        except ValueError:
            continue  # skip rows with non-numeric data

# Split into independent traces (tolerant to floating point error)
traces = []
current_trace = []
EPS = 1e-9

for x, y in data:
    current_trace.append((x, y))
    if abs(x - 1.0) < EPS:
        traces.append(current_trace)
        current_trace = []

if current_trace:
    traces.append(current_trace)

# --- First plot: linear scale ---
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    plt.plot(xs, ys, marker='o', linestyle='-', label=f"Trace {i}")

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data")
plt.title("Repeated Measurements vs Incrementer Value (Linear Y)")
plt.grid(True)
plt.legend()

# --- Second plot: log scale ---
plt.figure()
for i, trace in enumerate(traces, start=1):
    xs, ys = zip(*trace)
    plt.plot(xs, ys, marker='o', linestyle='-', label=f"Trace {i}")

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data (log scale)")
plt.title("Repeated Measurements vs Incrementer Value (Log Y)")
plt.yscale("log")
plt.grid(True, which="both", linestyle="--", linewidth=0.5)
plt.legend()

# Show both plots
plt.show()
