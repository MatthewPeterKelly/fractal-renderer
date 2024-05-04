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
# Windows:
cargo run --release  --  driven-damped-pendulum-render  .\examples\ddp_render\default.json
# Linux:
cargo run --release  --  driven-damped-pendulum-render  ./examples/ddp_render/default.json
```
## Testing notes:

Example, run the histogram test with outputs:
```
cargo test --test histogram
cargo test --test mandelbrot_core
```