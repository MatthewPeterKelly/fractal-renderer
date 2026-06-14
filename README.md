# Fractal Renderer

A utility for quickly rendering high-quality fractal images.

Currently this library supports six different fractals:

- [Mandelbrot Set](https://en.wikipedia.org/wiki/Mandelbrot_set)
- [Julia Set](https://en.wikipedia.org/wiki/Julia_set) (for the 𝑝(𝑧) = 𝑧² + 𝑐 quadratic map)
- [Barnsley Fern](https://en.wikipedia.org/wiki/Barnsley_fern)
- Attractor of the Driven-Damped Pendulum
- Generalized [Sierpiński Triangle](https://en.wikipedia.org/wiki/Sierpi%C5%84ski_triangle) to support N-sided polygons
- [Newton's Method](https://en.wikipedia.org/wiki/Newton_fractal) fractals (roots of unity and cosh 𝑧 − 1)

The binary produced by this project primarily supports two modes of operation:

- `render` -- used to render a single image directly to an output file
- `explore` -- opens an interactive GUI to pan and zoom around the fractal

## Gallery

**Driven-Damped Pendulum**

Visualization of the basin of attraction for the driven-damped pendulum, where each period in the fractal is one full revolution of the pendulum. Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/130#issuecomment-2705358520).

![driven-damped-pendulum-zoomed-out](https://github.com/user-attachments/assets/20dd0df6-aa3b-418a-a33c-bae1b765c9a3)

**Barnsley Fern**

Visualization of the Barnsley Fern, with the render settings tweaked so that it appears to be shadowed.
Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/130#issuecomment-2705371473).

![barnsley-fern-shadow-version](https://github.com/user-attachments/assets/ce91830c-b539-4989-a267-fcb719b48e59)

**Mandelbrot Set**

Visualization of the Mandelbrot Set with a dark blue color map and zoomed in a bit. Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/132#issuecomment-2706158399).

![mandelbrot-zoomed-in-dark](https://github.com/user-attachments/assets/4addea8e-6bf9-44d1-be34-527fc5fa2883)

**Sierpiński "Triangle"**

Visualization for the Sierpiński fractal, but generalized to an N-sided polygon. There are many ways to construct this fractal. This approach is implemented by sampling points from a sequence. Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/134#issuecomment-2726445693).

![serpinsky](https://github.com/user-attachments/assets/04112cf1-37f3-4671-87cc-622b746f39f6)

**Julia Quadratic Map**

Visualization of the Julia set. Source [here](https://github.com/MatthewPeterKelly/fractal-renderer/pull/133#issuecomment-2707873840).

![brassicas](https://github.com/user-attachments/assets/ff6b465a-d37c-4700-86ee-7a2b7134c369)

## Status: Active Development

This library is under active development, with plans to add support for more fractals and features over time.

**Render Mode:**

The `render` mode of operation is well developed -- it can be used right now to quickly generate high-quality fractal renders.

**Explore Mode:**

The `explore` mode enables the user to "fly around exploring the fractal" using the arrow keys to pan and WASD to adjust the instantaneous zoom rate. It supports the Mandelbrot set, Julia set, driven-damped pendulum, and Newton's method fractals. There is also a side-panel for live editing of the color map: a color picker, dynamically adding and removing keyframes, dragging to adjust the width of each gradient segment, and setting the background color used for in-set cells.

The color map edits operate on the cached scalar fields from the fractal, so they are super responsive. During interactive pan and zoom operations, the GUI will dynamically adjust the resolution and solve parameters, attempting to hit a 30 FPS render rate. As soon as interaction is done, it will progressively scale up to full quality renders.

Hitting spacebar during explore mode forces a full-quality render and writes it to file, along with a complete parameter set that reproduces the current view and color map.

## Examples

This project includes a large collection of examples under the `examples/` directory, covering both `render-*` and `explore-*` modes of operation, across all of the various types of fractals (`*-mandelbrot-*`, `*-julia-*`, `*-driven-damped-pendulum-*`, `*-newton-*`, ...). Each example is a Cargo example: a directory containing a lightweight `main.rs` wrapper plus a `params.json` file. The wrapper just loads the parameters and calls into the library to do the heavy lifting. To list all available examples, run `cargo run --example` with no name.

Many of the examples, especially the driven-damped pendulum, are computationally intensive, so it is usually a good idea to run them with the `--release` flag. This project defines a cargo alias, `rex` (`run --release --example`), so `cargo rex <name>` runs an example in release mode.

**Render Mode**

Run an example by name. These render a single image to file:

```
cargo rex render-mandelbrot-default
cargo rex render-julia-spiral
cargo rex render-driven-damped-pendulum
cargo rex render-barnsley-fern
cargo rex render-serpinksy-triangle
cargo rex render-newton-roots-of-unity-4
```

**Explore Mode:**

Explore mode opens a GUI and immediately renders the fractal with the specified parameters:

```
cargo rex explore-mandelbrot-default
cargo rex explore-julia-spiral
cargo rex explore-newton-cosh-minus-one
cargo rex explore-driven-damped-pendulum-quickly
```

You can interact with the GUI in the following ways:

- `a`/`d`: rapid zoom
- `w`/`s`: standard zoom
- arrow keys: pan
- click: pan to center window on selected point
- `r`: reset the view and color map to their initial state
- `q` (or `Ctrl+C`): close the GUI
- `space`: force a full-quality render and write it to file along with a complete JSON parameter set
- click a keyframe to select it; `Delete` removes the selected keyframe and `Esc` clears the selection

When actively interacting with the fractal, it renders in "fast mode" at a lower resolution. Once interaction stops, it renders at progressively higher quality, stopping at the original parameters. User events received during a render are condensed and processed once the render completes.

Note that `explore` mode does not support the Barnsley fern or Sierpiński triangle.

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

JSON and Markdown formatting uses [Prettier](https://prettier.io/). Run `npm install` once to install it, then `npm run fmt` to auto-format or `npm run fmt:check` to verify. Use a current Node.js release with a recent npm version compatible with the committed `package-lock.json`; if `npm install` fails on an older setup, upgrade npm (or Node.js, which bundles npm). Rust code is formatted and linted with the standard tooling (`cargo fmt`, `cargo clippy`).

## Acknowledgements

The interactive explore-mode GUI is built on the excellent [`eframe`/`egui`](https://github.com/emilk/egui) immediate-mode UI toolkit, which made the live color-map editor and render window straightforward to build.
