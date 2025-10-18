#[path = "../common/mod.rs"]
mod common;

/// Run the default example for rendering the Julia set.
/// ```sh
/// cargo run --example render-julia-spiral
/// ```
fn main() {
    common::render_example_from_string("render-julia-spiral")
}
