#[path = "../common.rs"]
mod common;

/// Run the default example for rendering the mandelbrot set.
/// ```sh
/// cargo run --example render-mandelbrot
/// ```
pub fn main() {
    common::render_example_from_string("render-mandelbrot")
}
