use crate::core::{
    color_map::{ColorMap, ColorMapLookUpTable, ColorMapper, LinearInterpolator},
    file_io::{serialize_to_json_or_panic, FilePrefix},
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::{
        generate_scalar_image, write_image_to_file_or_panic, ImageSpecification, PixelMapper,
    },
    lookup_table::LookupTable,
};
use image::Rgb;
use serde::{Deserialize, Serialize};

use crate::core::stopwatch::Stopwatch;

use super::quadratic_map::{ColorMapParams, QuadraticMapSequence};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    pub image_specification: ImageSpecification,
    // Convergence criteria
    pub escape_radius_squared: f64,
    pub max_iter_count: u32,
    pub refinement_count: u32,
    // All details related to coloring:
    pub color_map: ColorMapParams,
}

const ZERO_INITIAL_POINT: [f64; 2] = [0.0,0.0];

pub fn mandelbrot_pixel_renderer(
    params: &MandelbrotParams,
) -> (
    impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync,
    Histogram,
    CumulativeDistributionFunction,
) {
    let escape_radius_squared = params.escape_radius_squared;
    let max_iter_count = params.max_iter_count;
    let refinement_count = params.refinement_count;
    let background_color = Rgb(params.color_map.background_color_rgb);

    /////////////////////////////////////////////////////////////////////////

    // Create a reduced-resolution pixel map for the histogram samples:
    let hist_image_spec = params
        .image_specification
        .scale_to_total_pixel_count(params.color_map.histogram_sample_count as i32);

    let mut histogram = Histogram::new(
        params.color_map.histogram_bin_count,
        QuadraticMapSequence::log_iter_count(params.max_iter_count as f32),
    );
    let pixel_mapper = PixelMapper::new(&hist_image_spec);

    for i in 0..hist_image_spec.resolution[0] {
        let x = pixel_mapper.width.map(i);
        for j in 0..hist_image_spec.resolution[1] {
            let y = pixel_mapper.height.map(j);
            let maybe_value = QuadraticMapSequence::normalized_log_escape_count(
                &ZERO_INITIAL_POINT,
                &[x, y],
                escape_radius_squared,
                max_iter_count,
                refinement_count,
            );

            if let Some(value) = maybe_value {
                histogram.insert(value);
            }
        }
    }

    // Now compute the CDF from the histogram, which will allow us to normalize the color distribution
    let cdf = CumulativeDistributionFunction::new(&histogram);

    let base_color_map = ColorMap::new(&params.color_map.keyframes, LinearInterpolator {});

    let color_map = ColorMapLookUpTable {
        table: LookupTable::new(
            [cdf.min_data, cdf.max_data],
            params.color_map.lookup_table_count,
            |query: f32| {
                let mapped_query = cdf.percentile(query);
                base_color_map.compute_pixel(mapped_query)
            },
        ),
    };

    (
        move |point: &nalgebra::Vector2<f64>| {
            let maybe_value = QuadraticMapSequence::normalized_log_escape_count(
                &ZERO_INITIAL_POINT,
                &[point[0], point[1]],
                escape_radius_squared,
                max_iter_count,
                refinement_count,
            );
            if let Some(value) = maybe_value {
                color_map.compute_pixel(value)
            } else {
                background_color
            }
        },
        histogram,
        cdf,
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
