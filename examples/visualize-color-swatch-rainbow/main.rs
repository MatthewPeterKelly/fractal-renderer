#[path = "../common/mod.rs"]
mod common;

/// The `color_swatch` utility is used to enable developers to visualize the colors
/// in the colormap keyframes. I wrote it originally to visualize differences between
/// interpolation techniques for the color map. After trying a bunch of things, I kept
/// the simple linear interpolation, as the advanced techniques slowed down the rendering
/// and did not noticable improve the quality of the images.
/// ```sh
/// cargo run --example visualize-color-swatch-rainbow
/// ```
fn main() {
    common::color_swatch_example_from_string("visualize-color-swatch-rainbow")
}
