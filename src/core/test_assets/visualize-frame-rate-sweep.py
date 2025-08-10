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

# Split into independent traces
traces = []
current_trace = []

for x, y in data:
    current_trace.append((x, y))
    if x == 1.0:
        traces.append(current_trace)
        current_trace = []

# In case the file doesn't end exactly at x=1.0
if current_trace:
    traces.append(current_trace)

# Plot all traces
plt.figure()
for trace in traces:
    xs, ys = zip(*trace)
    plt.plot(xs, ys, marker='o', linestyle='-', label=f"Trace {len(plt.gca().lines)+1}")

plt.xlabel("Cyclic Incrementer Output")
plt.ylabel("Measured Data")
plt.title("Repeated Measurements vs Incrementer Value")
plt.grid(True)
plt.legend()
plt.show()
