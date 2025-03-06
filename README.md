# Fractal Renderer

A high-performance utility for rendering fractal images.

Currently this library supports five different fractals:

- [Mandelbrot Set](https://en.wikipedia.org/wiki/Mandelbrot_set)
- [Julia Set](https://en.wikipedia.org/wiki/Julia_set) (for the 𝑝(𝑧) = 𝑧² + 𝑐 quadratic map)
- [Barnsley Fern](https://en.wikipedia.org/wiki/Barnsley_fern)
- Attractor of the Driven-Damped Pendulum
- Generalized Serpinsky Triangle

The binary produced by this project primarily supports two modes of operation:

- `render` -- used to render a single image directly to an output file
- `explore` -- opens an interactive GUI to pan and zoom around the fractal

## Gallery

TODO

## Status: Active Development

This library is under active development, with plans to add support for more fractals and features over time.

**Render Mode:**

The `render` mode of operation is well developed -- it can be used right now to quickly generate high-quality fractal renders.

**Explore Mode:**

The `explore` mode is still a bit of a work-in-progress, but the prototype is fun to play around with!

It is quite responsive for both the Mandelbrot and Julia Sets, provided you run with reasonable settings and don't fill up the screen with "max iteration" data. The Driven-Damped Pendulum is much more computationally expensive, so that one is a bit laggy.

I'm actively experimenting with algorithms to dynamically adjust the render parameters for each fractal to hit a target frame rate of 30 Hz.

## Examples

Both the `explore` and `render` modes of operation accept the same JSON file format as input, which describes the fractal to be rendered. This JSON file is loaded by `serde` into the `common::FractalParams` enum, which tells the program what fractal to render, along with what parameters to use.

The examples listed below are designed to run relatively quickly. Within each `examples/*` subdirectory you'll find several other parameter files that generate some interesting renders.

**Render Mode**

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

**Explore Mode:**

Explore mode will open a GUI and immediately render the fractal with the specified parameters.

You can interact with the GUI in the following ways:

- `a`/`d`: rapid zoom
- `w`/`s`: standard zoom
- arrow keys: pan
- click: pan to center window on selected point
- `r` reset to initial view
- `esc` close the GUI
- `space` write image to file along with (partial) JSON params

When actively interacting with the fractal, it will render in "fast mode", at a lower resolution. Once interaction has stopped, it will render at progressively higher quality, stopping at the original paramters. This feature is still experimental.

User events received during rendering will be condensed and processed after rendering. Eventually I plan to add the ability to interrupt a slow render with a GUI event.

```
cargo run --release -- explore ./examples/mandelbrot/default.json
```

```
cargo run --release -- explore ./examples/julia/default.json
```

```
cargo run --release -- explore ./examples/driven_damped_pendulum/ddp_low_res_antialias.json
```

**Color-Swatch Mode**

The simple "color-swatch" mode is used for debugging and tweaking color map data. It has a slightly different input format. Eventually I would love to replace it with an interactive GUI with a color-picker... but that is low-priority for now.

```
cargo run  --release -- color-swatch examples/color_swatch/rainbow.json
```

## Software Architecture Overview

TODO

## Testing

**Unit Testing**

Much of the core library and examples are covered by unit tests, although the coverage is not strict.

**Integration Tests**

The few tests that render full images (at low resolution), then read them back in and compare a hash of the image data.

There is a test to ensure that all of the `.json` files in `examples` are able to be parsed into valid rust enums.

## Developer Notes

JSON and Markdown formatting via [prettier](https://prettier.io/). Rust code is formatted and linted with the standard tooling.

## Acknowledgements

Thanks to the excellent example from the [pixel.rs](https://docs.rs/pixels), which was really helpful in getting the GUI working:
https://github.com/parasyte/pixels/tree/39e84aacbe117347e7b8e7201c48184344aed9cc/examples/conway
