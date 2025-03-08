# Fractal Renderer

WORK IN PROGRESS

Initial goal: write a simple CLI for generating high-quality images of the mandelbrot set.

Long-term goal: add support for zoom sequences and other fractals, along with (maybe?) a browser interface.

## Usage Examples:

### Render Command:

```
cargo run --release -- render ./examples/mandelbrot/default.json
```
```
cargo run --release -- render ./examples/julia/default.json
```

```
cargo run --release -- render ./examples/driven_damped_pendulum/default.json
```

```
cargo run --release -- render ./examples/barnsley_fern/default.json
```

```
cargo run --release -- render ./examples/serpinsky/triangle.json
```

```
cargo run  --release -- color-swatch examples/color_swatch/rainbow.json
```

### Explore Command

```
cargo run --release -- explore ./examples/mandelbrot/default.json
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

## Acknowledgements

Thanks to the excellent example from the [pixel.rs](https://docs.rs/pixels), which was really helpful in getting the GUI working:
https://github.com/parasyte/pixels/tree/39e84aacbe117347e7b8e7201c48184344aed9cc/examples/conway
