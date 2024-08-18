use serde::{Deserialize, Serialize};

use crate::core::{
    color_map::{
        with_uniform_spacing, ColorMap, ColorMapKeyFrame, ColorMapper, LinearInterpolator,
        StepInterpolator,
    },
    file_io::{serialize_to_json_or_panic, FilePrefix},
    image_utils::write_image_to_file_or_panic,
    stopwatch::{self, Stopwatch},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ColorSwatchParams {
    pub swatch_resolution: (u32, u32),
    pub border_padding: u32,
    pub border_color_rgb: [u8; 3],
    pub keyframes: Vec<ColorMapKeyFrame>,
}

pub struct NamedColorMapper {
    pub name: String,
    pub color_map: Box<dyn ColorMapper>,
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

    let mut stopwatch = Stopwatch::new("Color Swatch".to_owned());

    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);

    let uniform_keyframes = with_uniform_spacing(&params.keyframes);
    let color_maps: Vec<NamedColorMapper> = vec![
        NamedColorMapper {
            name: "user-defined, linear interpolation".to_owned(),
            color_map: Box::new(ColorMap::new(&params.keyframes, LinearInterpolator {})),
        },
        NamedColorMapper {
            name: "user-defined, nearest interpolation".to_owned(),
            color_map: Box::new(ColorMap::new(
                &params.keyframes,
                StepInterpolator { threshold: 0.5 },
            )),
        },
        NamedColorMapper {
            name: "uniform-spacing, linear interpolation".to_owned(),
            color_map: Box::new(ColorMap::new(&uniform_keyframes, LinearInterpolator {})),
        },
        NamedColorMapper {
            name: "uniform-spacing, nearest interpolation".to_owned(),
            color_map: Box::new(ColorMap::new(
                &uniform_keyframes,
                StepInterpolator { threshold: 0.5 },
            )),
        },
    ];

    stopwatch.record_split("setup color maps".to_owned());

    // Save the image to a file, deducing the type from the file name
    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = {
        let total_width = 2 * params.border_padding + params.swatch_resolution.0;
        let total_height = (color_maps.len() as u32)
            * (params.border_padding + params.swatch_resolution.1)
            + params.border_padding;
        image::ImageBuffer::new(total_width, total_height)
    };

    let x_offset = params.border_padding;
    let mut y_offset = params.border_padding;
    let scale = 1.0 / ((params.swatch_resolution.0 * params.swatch_resolution.1) as f32);

    stopwatch.record_split("setup image buffer".to_owned());

    for named_map in color_maps {
        for x_idx in 0..params.swatch_resolution.0 {
            for y_idx in 0..params.swatch_resolution.1 {
                let linear_index = x_idx * params.swatch_resolution.1 + y_idx;
                *imgbuf.get_pixel_mut(x_idx + x_offset, y_idx + y_offset) = named_map
                    .color_map
                    .compute_pixel(scale * (linear_index as f32));
            }
        }
        y_offset += params.swatch_resolution.1 + params.border_padding;

        stopwatch.record_split(format!("evaluate color map:  {}", named_map.name.clone()));
    }

    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });

    stopwatch.record_split("write image file".to_owned());

    stopwatch
        .display(&mut file_prefix.create_file_with_suffix("_diagnostics.txt"))
        .unwrap();
}
