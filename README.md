# Fractal Renderer

WORK IN PROGRESS

Initial goal: write a simple CLI for generating high-quality images of the mandelbrot set.

Long-term goal: add support for zoom sequences and other fractals, along with (maybe?) a browser interface.

## Usage (Windows; flip path delimerer to `/` for unix-based)

```
cargo run --release -- render .\examples\mandelbrot\default.json
```

```
cargo run --release -- render .\examples\driven_damped_pendulum\default.json
```

```
cargo run --release -- render .\examples\driven_damped_pendulum\default_series.json
```

```
cargo run --release -- render .\examples\barnsley_fern\default.json
```

```
cargo run --release -- render .\examples\serpinsky\triangle.json
```

```
cargo run --release -- explore .\examples\mandelbrot\default.json
```

```
cargo run color-swatch -- examples\color_swatch\wikipedia.json
```

## Autoformatting:

### Rust Code:

```
cargo fmt
```

```
cargo clippy --fix
```

### JSON

Use the Prettier extension for VSCode.

## Testing notes:

Example, run the histogram test with outputs:

```
cargo test --lib histogram
```

To run all tests in the library:

```
cargo test --lib
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

## Acknowledgements

Thanks to the excellent example from the [pixel.rs](https://docs.rs/pixels), which was really helpful in getting the GUI working.

https://github.com/parasyte/pixels/tree/39e84aacbe117347e7b8e7201c48184344aed9cc/examples/conway
