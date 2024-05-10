# Fractal Renderer

WORK IN PROGRESS

Initial goal:  write a simple CLI for generating high-quality images of the mandelbrot set.

Long-term goal:  add support for zoom sequences and other fractals, along with (maybe?) a browser interface.

## Usage:

```
cargo run --release --  mandelbrot-render .\examples\mandelbrot_render\complete.json
```

```
cargo run --release -- mandelbrot-search .\examples\mandelbrot_search\default.json
```

```
cargo run --release  --  driven-damped-pendulum-render  .\examples\ddp_render\default.json
```
## Testing notes:

Example, run the histogram test with outputs:
```
cargo test --test histogram
cargo test --test mandelbrot_core
```

## Windows Rust Dummy Notes

Stack Trace:
```
$env:RUST_BACKTRACE=1; cargo run
```

## Rendering an image series to an animation:
Run this from the output directory where the images are:
```
ffmpeg -framerate 30 -i high_res_series_%d.png -c:v libx264 -profile:v high -crf 20 -pix_fmt yuv420p high_res_series.mp4
```