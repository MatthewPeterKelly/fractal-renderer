#[path = "../common/mod.rs"]
mod common;

/// Render the Barnsley fern with the shadow effect, but at QHD resolution
/// with all of the other parameters cranked up. The result is beaufitul, but
/// it takes a long time to render.
/// ```sh
/// cargo run --example render-barnsley-shadow-fern-QHD
/// ```
pub fn main() {
    common::render_example_from_string("render-barnsley-shadow-fern-QHD")
}
