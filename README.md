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
Use `ffmpeg` to render the animation. Here is one example call:
```
ffmpeg -framerate 16 -i out/ddp_render/default_series/series/default_series_%d.png -c:v libx264 -profile:v high -crf 20 -pix_fmt yuv420p out/default_series.mp4
```

Then to go one step furher and make it into a looping gif:
```
ffmpeg -i out/default_series.mp4 out/default_series.gif
```
