#[path = "../common/mod.rs"]
mod common;

/// Render the fractal basin of attractions of the expression `cosh(z)-1`.
/// ```sh
/// cargo rex render-newton-cosh-minus-one
/// ```
fn main() {
    common::render_example_from_string("render-newton-cosh-minus-one")
}
