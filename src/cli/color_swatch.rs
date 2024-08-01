use serde::{Deserialize, Serialize};

use crate::core::{
    color_map::{ColorMapKeyFrame, InterpolationMode, PiecewiseLinearColorMap},
    file_io::{build_output_path_with_date_time, FilePrefix},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ColorSwatchParams {
    pub swatch_resolution: (u32, u32),
    pub border_padding: u32,
    pub border_rgb: [u8; 3],
    pub keyframes: Vec<ColorMapKeyFrame>,
}

pub fn generate_color_swatch(params_path: &str) {
    let params: ColorSwatchParams = serde_json::from_str(
        &std::fs::read_to_string(params_path).expect("Unable to read param file"),
    )
    .unwrap();

    let file_prefix = FilePrefix {
        directory_path: build_output_path_with_date_time("color_swatch", "debug", &None),
        file_base: "colors".to_owned(), // HACK!!!!
    };

    std::fs::write(
        file_prefix.with_suffix(".json"),
        serde_json::to_string(&params).unwrap(),
    )
    .expect("Unable to write file");

    // We'll visualize each of these in the same image output file
    let interpolation_modes = vec![InterpolationMode::Direct, InterpolationMode::Srgb, InterpolationMode::Hsl];

    // Save the image to a file, deducing the type from the file name
    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = {
        let total_width = 2 * params.border_padding + params.swatch_resolution.0;
        let total_height = (interpolation_modes.len() as u32)
            * (params.swatch_resolution.1 + params.border_padding)
            + params.border_padding;
        image::ImageBuffer::new(total_width, total_height)
    };

    let colormap = PiecewiseLinearColorMap::new(params.keyframes);

    let x_offset = params.border_padding;
    let mut y_offset = params.border_padding;
    let scale = 1.0 / ((params.swatch_resolution.0 * params.swatch_resolution.1) as f32);

    for interpolation_mode in interpolation_modes {
        for x_idx in x_offset..(x_offset + params.swatch_resolution.0) {
            for y_idx in y_offset..(y_offset + params.swatch_resolution.1) {
                let linear_index = x_idx * params.swatch_resolution.1 + y_idx;

                // TODO:  bug -- we're putting something bad into the top here.
                *imgbuf.get_pixel_mut(x_idx, y_idx) =
                    image::Rgb(colormap.compute(scale * (linear_index as f32), interpolation_mode));
            }
        }
        y_offset += params.border_padding + params.swatch_resolution.1;
    }

    let render_path = file_prefix.with_suffix(".png");
    imgbuf.save(&render_path).unwrap();
    println!("INFO:  Wrote image file to: {}", render_path.display());
}
