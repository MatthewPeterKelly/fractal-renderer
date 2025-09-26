#[path = "../common/mod.rs"]
mod common;

/// ```sh
/// cargo run --example render-serpinksy-pentagon
/// ```
pub fn main() {
    common::render_example_from_string("render-serpinksy-pentagon")
}
