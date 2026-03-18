#[path = "../common/mod.rs"]
mod common;

/// Demo: color-map editor UI alongside a live fractal preview.
///
/// Opens two windows side by side: the fractal preview renders in the
/// background while the editor panel shows the gradient bar and a text
/// overlay drawn with tiny-skia + fontdue (Hack Regular).
///
/// ```sh
/// cargo rex color-map-editor-demo
/// ```
fn main() {
    common::color_map_editor_example_from_string("color-map-editor-demo")
}
