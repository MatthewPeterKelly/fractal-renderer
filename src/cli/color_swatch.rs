use serde::{Deserialize, Serialize};

use crate::core::{
    color_map::{ColorMapKeyFrame, PiecewiseLinearColorMap},
    file_io::{serialize_to_json_or_panic, FilePrefix},
    image_utils::write_image_to_file_or_panic,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ColorSwatchParams {
    pub swatch_resolution: (u32, u32),
    pub border_padding: u32,
    pub border_color_rgb: [u8; 3],
    pub keyframes: Vec<ColorMapKeyFrame>,
}

/**
 * Generates a "color swatch" that makes it easier to visualize color maps.
 * -- user spacing, with no interpolation
 * -- user spacing, with linear interpolation
 * -- uniform spacing, with no interpolation
 * -- uniform spacing, with linear interpolation
 */
pub fn generate_color_swatch(params_path: &str, file_prefix: FilePrefix) {
    let params: ColorSwatchParams = serde_json::from_str(
        &std::fs::read_to_string(params_path).expect("Unable to read param file"),
    )
    .unwrap();

    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);

    // Save the image to a file, deducing the type from the file name
    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = {
        let total_width = 2 * params.border_padding + params.swatch_resolution.0;
        let total_height =
            4 * (params.border_padding + params.swatch_resolution.1) + params.border_padding;
        image::ImageBuffer::new(total_width, total_height)
    };

    let user_colormap = PiecewiseLinearColorMap::new(params.keyframes);
    let uniform_color_map = user_colormap.with_uniform_spacing();

    let x_offset = params.border_padding;
    let mut y_offset = params.border_padding;
    let scale = 1.0 / ((params.swatch_resolution.0 * params.swatch_resolution.1) as f32);

    for color_map in [user_colormap, uniform_color_map] {
        for clamp_to_nearest in [true, false] {
            for x_idx in x_offset..(x_offset + params.swatch_resolution.0) {
                for y_idx in y_offset..(y_offset + params.swatch_resolution.1) {
                    let linear_index = x_idx * params.swatch_resolution.1 + y_idx;
                    *imgbuf.get_pixel_mut(x_idx, y_idx) = image::Rgb(
                        color_map.compute(scale * (linear_index as f32), clamp_to_nearest),
                    );
                }
            }
            y_offset += params.swatch_resolution.1 + params.border_padding;
        }
    }

    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });
}
