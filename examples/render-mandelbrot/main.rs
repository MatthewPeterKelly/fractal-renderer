use fractal_renderer::cli::render::render_example_from_string;

/// Run the default example for rendering the mandelbrot set.
/// ```sh
/// cargo run --example render-mandelbrot
/// ```
pub fn main() {
    render_example_from_string("render-mandelbrot")
}
