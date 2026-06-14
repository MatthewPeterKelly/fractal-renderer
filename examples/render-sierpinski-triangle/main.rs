#[path = "../common/mod.rs"]
mod common;

/// ```sh
/// cargo run --example render-sierpinski-triangle
/// ```
fn main() {
    common::render_example_from_string("render-sierpinski-triangle")
}
