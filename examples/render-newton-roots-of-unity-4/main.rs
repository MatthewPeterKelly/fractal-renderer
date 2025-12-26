#[path = "../common/mod.rs"]
mod common;

/// Render the fourth-order "roots of unity" fractal
/// ```sh
/// cargo rex render-newton-roots-of-unity-4
/// ```
fn main() {
    common::render_example_from_string("render-newton-roots-of-unity-4")
}
