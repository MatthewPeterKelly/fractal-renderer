# Fractal Renderer

WORK IN PROGRESS

Initial goal:  write a simple CLI for generating high-quality images of the mandelbrot set.

Long-term goal:  add support for zoom sequences and other fractals, along with (maybe?) a browser interface.

## Usage:

```
cargo run -- mandelbrot .\examples\mandelbrot\default_params.json
```

```
cargo run -- mandelbrot-search .\examples\mandelbrot_search\default_params.json
```

## TODO:
- Build out README
- Add test to ensure that the above example works (store hash of the file, then check that in CI).


## Driven-Damped Pendulum

### Trace Visualizer:
One idea is to run simulations and then add the entire simulation to the image if it converges. This could dramatically speed up computation,
because we wouldn't need to sample the grid as densely. We would need to be somewhat clever to avoid artifacts due to sampling method.
- Every pixel that is traversed by a convergent siulation is flipped to "on". Iterate through every pixel, skipping ones that diverge.
  - we need to be acareful here to avoid biasing the image, as there will be points within a single pixel that actually do not converge.
  - perhaps a better way to think of the image is as a grid of dots (or points), and we only mark a pixel as "on" if the trace passes within some
   minimum radius of that dot.
  - it imght be that we can reduce the total number of calculations, but it also might be that the point-line distance checks are slower than just doing every
    calculation from scratch at the center of each pixel, which is easier and will produce a consistent image
- conclusion: better to just run a simulation from every point.

