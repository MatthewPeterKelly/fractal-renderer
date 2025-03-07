# Fractal Renderer

A high-performance utility for rendering fractal images.

Currently this library supports five different fractals:

- [Mandelbrot Set](https://en.wikipedia.org/wiki/Mandelbrot_set)
- [Julia Set](https://en.wikipedia.org/wiki/Julia_set) (for the 𝑝(𝑧) = 𝑧² + 𝑐 quadratic map)
- [Barnsley Fern](https://en.wikipedia.org/wiki/Barnsley_fern)
- Attractor of the Driven-Damped Pendulum
- Generalized [Serpinsky Triangle](https://en.wikipedia.org/wiki/Sierpi%C5%84ski_triangle) to support N-sided polygons

The binary produced by this project primarily supports two modes of operation:

- `render` -- used to render a single image directly to an output file
- `explore` -- opens an interactive GUI to pan and zoom around the fractal

## Gallery

**Driven-Damped Pendulum**

Visualization of the basin of attraction for the driven-damped pendulum, where each period in the fractal is one full revolution of the pendulum. Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/130#issuecomment-2705358520).

![ddp_QHD_with_antialias](https://github.com/user-attachments/assets/7af4ca16-7135-4a43-ae74-ee1985521130)

**Barnsley Fern**

Visualization of the Barnsley Fern, with the render settings tweaked so that it appears to be shadowed.
Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/130#issuecomment-2705371473).

![shadow_fern_QHD](https://github.com/user-attachments/assets/532b49f8-646c-48ea-8882-1644a98e105b)

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

Run in release mode, pass the `render` argument, followed by a JSON file path:

```
cargo run --release -- render ./examples/mandelbrot/default.json
cargo run --release -- render ./examples/julia/default.json
cargo run --release -- render ./examples/driven_damped_pendulum/default.json
cargo run --release -- render ./examples/barnsley_fern/default.json
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

When actively interacting with the fractal, it will render in "fast mode", at a lower resolution. Once interaction has stopped, it will render at progressively higher quality, stopping at the original parameters. This feature is still experimental.

User events received during rendering will be condensed and processed after rendering. Eventually I plan to add the ability to interrupt a slow render with a GUI event.

The calling pattern matches `render`, and it uses the same JSON files:

```
cargo run --release -- explore ./examples/mandelbrot/default.json
cargo run --release -- explore ./examples/julia/default.json
cargo run --release -- explore ./examples/driven_damped_pendulum/ddp_low_res_antialias.json
```

Note that `explore` mode does not support the barnsley fern or serpinsky triangle.

**Color-Swatch Mode**

The simple "color-swatch" mode is used for debugging and tweaking color map data. It has a slightly different input format.

```
cargo run  --release -- color-swatch examples/color_swatch/rainbow.json
```

## Software Design

The software for the fractal renderer was written with two goals in mind:

- Be clear, correct, and maintainable
- Render images as fast as possible

Working toward these goals:

- Most of the "inner loops" of the rendering pipeline are parallelized with Rayon
- Much of the core library and examples are covered by unit tests
- There are integration tests for full rendering pipeline
- Core library components are modular, documented, and shared between the different fractals.
- Generics are used extensively to achieve static polymorphism

## Developer Notes

Refer to [`CONTRIBUTING.md`](./CONTRIBUTING.md) file if you are interested in contributing to this project.

This project is covered by the MIT [LICENSE](./LICENSE).

JSON and Markdown formatting via [prettier](https://prettier.io/). Rust code is formatted and linted with the standard tooling.

## Acknowledgements

Thanks to the excellent example from the [pixel.rs](https://docs.rs/pixels), which was really helpful in getting the GUI working:
https://github.com/parasyte/pixels/tree/39e84aacbe117347e7b8e7201c48184344aed9cc/examples/conway
