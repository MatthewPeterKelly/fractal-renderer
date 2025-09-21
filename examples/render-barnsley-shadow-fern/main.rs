#[path = "../common/mod.rs"]
mod common;

/// Render the Barnsley fern, but increase the antialiasing substantially. This,
/// combined with a signifficant reduction in samples (normalized by subpixel count)
/// creates a shadow effect that looks quite nice.
/// ```sh
/// cargo run --example render-barnsley-shadow-fern
/// ```
fn main() {
    common::render_example_from_string("render-barnsley-shadow-fern")
}
