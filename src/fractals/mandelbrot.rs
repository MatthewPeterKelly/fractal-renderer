use crate::core::{
    file_io::{serialize_to_json_or_panic, FilePrefix},
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::{
        generate_scalar_image, write_image_to_file_or_panic, ImageSpecification,
    },
};
use image::Rgb;
use serde::{Deserialize, Serialize};

use crate::core::stopwatch::Stopwatch;

use super::quadratic_map::{pixel_renderer, ColorMapParams, ConvergenceParams, QuadraticMapSequence};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    pub image_specification: ImageSpecification,
    pub convergence_params: ConvergenceParams,
    pub color_map: ColorMapParams,
}

const ZERO_INITIAL_POINT: [f64; 2] = [0.0, 0.0];

pub fn mandelbrot_pixel_renderer(
    params: &MandelbrotParams,
) -> (
    impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync,
    Histogram,
    CumulativeDistributionFunction,
) {
    let convergence_params = params.convergence_params.clone();
    pixel_renderer(&params.image_specification, &params.color_map,
            move |point: &[f64; 2]| {
                QuadraticMapSequence::normalized_log_escape_count(
                    &ZERO_INITIAL_POINT,
                    point,
                    &convergence_params,
                )
        }, QuadraticMapSequence::log_iter_count(params.convergence_params.max_iter_count as f32),
    )
}

pub fn render_mandelbrot_set(
    params: &MandelbrotParams,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch = Stopwatch::new("Mandelbrot Render Stopwatch".to_owned());

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        params.image_specification.resolution[0],
        params.image_specification.resolution[1],
    );

    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);

    stopwatch.record_split("basic setup".to_owned());

    let (pixel_renderer, histogram, cdf) = mandelbrot_pixel_renderer(params);

    stopwatch.record_split("build renderer".to_owned());

    let raw_data =
        generate_scalar_image(&params.image_specification, pixel_renderer, Rgb([0, 0, 0]));

    stopwatch.record_split("compute mandelbrot sequence".to_owned());

    // Apply color to each pixel in the image:
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = raw_data[x as usize][y as usize];
    }

    stopwatch.record_split("copy into image buffer".to_owned());
    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });
    stopwatch.record_split("write PNG".to_owned());

    let mut diagnostics_file = file_prefix.create_file_with_suffix("_diagnostics.txt");
    stopwatch.display(&mut diagnostics_file)?;
    histogram.display(&mut diagnostics_file)?;
    cdf.display(&mut diagnostics_file)?;

    Ok(())
}
