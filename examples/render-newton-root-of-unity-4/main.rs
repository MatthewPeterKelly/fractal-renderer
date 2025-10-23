#[path = "../common/mod.rs"]
mod common;

/// Slightly more-expensive rendering of the Julia set, producing a flower-like pattern.
/// ```sh
/// cargo run --example render-julia-flower
/// ```
fn main() {
    common::render_example_from_string("render-newton-roots-of-unity-4")
}
