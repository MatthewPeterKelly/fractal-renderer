//! Demo: color map editor GUI with a Mandelbrot preview.
//!
//! Run with: `cargo run --example color-gui-demo`

#[path = "../common/mod.rs"]
mod common;

fn main() {
    common::color_editor_example_from_string("color-gui-demo");
}
