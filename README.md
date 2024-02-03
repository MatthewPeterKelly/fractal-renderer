# Fractal Renderer

WORK IN PROGRESS

Initial goal:  write a simple CLI for generating high-quality images of the mandelbrot set.

Long-term goal:  add support for zoom sequences and other fractals, along with (maybe?) a browser interface.

## Usage:

```
cargo run -- mandelbrot-render .\examples\mandelbrot_render\default_params.json
```

```
cargo run -- mandelbrot-search .\examples\mandelbrot_search\default_params.json
cargo run --release -- mandelbrot-search .\examples\mandelbrot_search\expensive_params.json
```

## Testing notes:

Example, run the histogram test with outputs:
```
cargo test test_histogram_insert  -- --nocapture
```