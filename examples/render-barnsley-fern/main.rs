#[path = "../common/mod.rs"]
mod common;

// Default example for rendering the Barnsley fern.
/// ```sh
/// cargo run --example render-barnsley-fern
/// ```
pub fn main() {
    common::render_example_from_string("render-barnsley-fern")
}
