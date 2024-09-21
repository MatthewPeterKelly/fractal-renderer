use std::io;

use crate::core::{
    color_map::{ColorMap, ColorMapKeyFrame, ColorMapLookUpTable, ColorMapper, LinearInterpolator},
    file_io::{serialize_to_json_or_panic, FilePrefix},
    histogram::{self, CumulativeDistributionFunction, Histogram},
    image_utils::{
        generate_scalar_image, write_image_to_file_or_panic, ImageSpecification, PixelMapper,
    },
    lookup_table::LookupTable,
};
use image::Rgb;
use serde::{Deserialize, Serialize};

use crate::core::stopwatch::Stopwatch;

#[derive(Serialize, Deserialize, Debug)]
pub struct ColorMapParams {
    pub keyframes: Vec<ColorMapKeyFrame>,
    pub lookup_table_count: usize,
    pub background_color_rgb: [u8; 3],
    pub histogram_bin_count: usize,
    pub histogram_sample_count: usize,
}

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

/**
 * Data structure for storing the internal state of the mandelbrot sequence calculation.
 * Highly optimized version of the equation to reduce floating point operation count.
 */
pub struct MandelbrotSequence {
    pub x0: f64,
    pub y0: f64,
    pub x_sqr: f64,
    pub y_sqr: f64,
    pub x: f64,
    pub y: f64,
    pub iter_count: u32,
}

impl MandelbrotSequence {
    fn new(point: &nalgebra::Vector2<f64>) -> MandelbrotSequence {
        let mut value = MandelbrotSequence {
            x0: point[0],
            y0: point[1],
            x_sqr: 0.0,
            y_sqr: 0.0,
            x: 0.0,
            y: 0.0,
            iter_count: 0,
        };
        value.step(); // ensures that cached values are correct
        value
    }

    fn radius_squared(&self) -> f64 {
        self.x_sqr + self.y_sqr
    }

    fn radius(&self) -> f64 {
        self.radius_squared().sqrt()
    }

    // Z = Z*Z + C
    fn step(&mut self) {
        self.y = (self.x + self.x) * self.y + self.y0;
        self.x = self.x_sqr - self.y_sqr + self.x0;
        self.x_sqr = self.x * self.x;
        self.y_sqr = self.y * self.y;
        self.iter_count += 1;
    }

    // @return: true -- escaped! false --> did not escape
    // @return: iteration count if the point escapes, otherwise None().
    fn step_until_condition(
        &mut self,
        max_iter_count: u32,
        max_radius_squared: f64,
    ) -> Option<f32> {
        while self.iter_count < max_iter_count {
            if self.radius_squared() > max_radius_squared {
                return Some(self.iter_count as f32);
            }
            self.step();
        }
        None
    }

    /**
     * @return: normalized iteration count (if escaped), or unset optional.
     */
    fn compute_normalized_escape(
        &mut self,
        max_iter_count: u32,
        max_radius_squared: f64,
        refinement_count: u32,
    ) -> Option<f32> {
        use std::f64;
        let _ = self.step_until_condition(max_iter_count, max_radius_squared);
        for _ in 0..refinement_count {
            self.step();
        }
        const SCALE: f64 = 1.0 / std::f64::consts::LN_2;
        let normalized_iteration_count =
            (self.iter_count as f64) - f64::ln(f64::ln(self.radius())) * SCALE;

        if normalized_iteration_count < max_iter_count as f64 {
            Some(normalized_iteration_count as f32)
        } else {
            None
        }
    }

    /// Test whether a point is in the mandelbrot set.
    /// @param test_point: a point in the complex plane to test
    /// @param escape_radius_squared: a point is not in the mandelbrot set if it exceeds this radius squared from the origin during the mandelbrot iteration sequence.
    /// @param max_iter_count: assume that a point is in the mandelbrot set if this number of iterations is reached without exceeding the escape radius.
    /// @param refinement_count: normalize the escape count, providing smooth interpolation between integer "escape count" values.
    /// @return: normalized (smooth) iteration count if the point escapes, otherwise None().
    pub fn normalized_escape_count(
        test_point: &nalgebra::Vector2<f64>,
        escape_radius_squared: f64,
        max_iter_count: u32,
        refinement_count: u32,
    ) -> Option<f32> {
        let mut escape_sequence = MandelbrotSequence::new(test_point);

        if refinement_count == 0 {
            return escape_sequence.step_until_condition(max_iter_count, escape_radius_squared);
        }

        escape_sequence.compute_normalized_escape(
            max_iter_count,
            escape_radius_squared,
            refinement_count,
        )
    }
}

pub fn mandelbrot_pixel_renderer(
    params: &MandelbrotParams,
) -> impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync {
    mandelbrot_pixel_renderer_with_hist(params, & mut Histogram::default())
}


pub fn mandelbrot_pixel_renderer_with_hist(
    params: &MandelbrotParams,
    histogram: &mut Histogram,
) -> impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync {
    let escape_radius_squared = params.escape_radius_squared;
    let max_iter_count = params.max_iter_count;
    let refinement_count = params.refinement_count;
    let background_color = Rgb(params.color_map.background_color_rgb);

    /////////////////////////////////////////////////////////////////////////

    let max_iteration_domain = params.max_iter_count as f32;

    // Create a reduced-resolution pixel map for the histogram samples:
    let hist_image_spec = params
        .image_specification
        .scale_to_total_pixel_count(params.color_map.histogram_sample_count as i32);

        *histogram= Histogram::new(
        params.color_map.histogram_bin_count,
        max_iteration_domain,
    );
    let pixel_mapper = PixelMapper::new(&hist_image_spec);

    for i in 0..hist_image_spec.resolution[0] {
        let x = pixel_mapper.width.map(i);
        for j in 0..hist_image_spec.resolution[1] {
            let y = pixel_mapper.height.map(j);
            let maybe_value = MandelbrotSequence::normalized_escape_count(
                &nalgebra::Vector2::new(x, y),
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
    let cdf = CumulativeDistributionFunction::new(histogram);

    let base_color_map = ColorMap::new(&params.color_map.keyframes, LinearInterpolator {});

    let color_map = ColorMapLookUpTable {
        table: LookupTable::new(
            [0.0, max_iteration_domain],
            params.color_map.lookup_table_count,
            |query: f32| {
                let mapped_query = cdf.percentile(query);
                base_color_map.compute_pixel(mapped_query)
            },
        ),
    };

    move |point: &nalgebra::Vector2<f64>| {
        let maybe_value = MandelbrotSequence::normalized_escape_count(
            point,
            escape_radius_squared,
            max_iter_count,
            refinement_count,
        );
        if let Some(value) = maybe_value {
            color_map.compute_pixel(value)
        } else {
            background_color
        }
    }
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

    let mut histogram = Histogram::default();
    let pixel_renderer = mandelbrot_pixel_renderer_with_hist(params, &mut histogram);

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

    Ok(())
}
