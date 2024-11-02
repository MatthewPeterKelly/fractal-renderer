use image::Rgb;
use serde::{Deserialize, Serialize};

use crate::core::{
    color_map::{ColorMap, ColorMapKeyFrame, ColorMapLookUpTable, ColorMapper, LinearInterpolator},
    file_io::{serialize_to_json_or_panic, FilePrefix},
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::{
        generate_scalar_image, write_image_to_file_or_panic, ImageSpecification, PixelMapper,
    },
    lookup_table::LookupTable,
    stopwatch::Stopwatch,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorMapParams {
    pub keyframes: Vec<ColorMapKeyFrame>,
    pub lookup_table_count: usize,
    pub background_color_rgb: [u8; 3],
    pub histogram_bin_count: usize,
    pub histogram_sample_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConvergenceParams {
    pub escape_radius_squared: f64,
    pub max_iter_count: u32,
    pub refinement_count: u32,
}

/**
 * Data structure for storing the internal state of the mandelbrot sequence calculation.
 * Highly optimized version of the equation to reduce floating point operation count.
 */
pub struct QuadraticMapSequence {
    pub x0: f64,
    pub y0: f64,
    pub x: f64,
    pub y: f64,
    pub x_sqr: f64,
    pub y_sqr: f64,
    pub iter_count: u32,
}

impl QuadraticMapSequence {
    /// Implements the equation:  Z := Z*Z + C, where both Z and C are imaginary numbers
    /// point:  initial value for "Z" in the above equation
    /// constant_term:  initial value for "C" in the above equation
    fn new(point: &[f64; 2], constant_term: &[f64; 2]) -> QuadraticMapSequence {
        let mut value = QuadraticMapSequence {
            x0: constant_term[0],
            y0: constant_term[1],
            x: point[0],
            y: point[1],
            x_sqr: point[0] * point[0],
            y_sqr: point[1] * point[1],
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

    // natural log of the iteration count, shifted to be on range (0,inf) for positive inputs
    pub fn log_iter_count(iter_count: f32) -> f32 {
        (iter_count - 1.0).ln()
    }

    // Z = Z*Z + C
    // Note:  This implementation is somewhat faster than the directly writing the above equation with the `Complex` number type.
    fn step(&mut self) {
        self.y = (self.x + self.x) * self.y + self.y0;
        self.x = self.x_sqr - self.y_sqr + self.x0;
        self.x_sqr = self.x * self.x;
        self.y_sqr = self.y * self.y;
        self.iter_count += 1;
    }

    // @return: true -- escaped! false --> did not escape
    // @return: true if the point escapes, false otherwise.
    fn step_until_condition(&mut self, max_iter_count: u32, max_radius_squared: f64) -> bool {
        while self.iter_count < max_iter_count {
            if self.radius_squared() > max_radius_squared {
                return true;
            }
            self.step();
        }
        false
    }

    /**
     * @return: natural log of the normalized iteration count (if escaped), or unset optional.
     */
    fn compute_normalized_log_escape(
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
            Some(Self::log_iter_count(normalized_iteration_count as f32))
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
    pub fn normalized_log_escape_count(
        test_point: &[f64; 2],
        constant_term: &[f64; 2],
        convergence_params: &ConvergenceParams,
    ) -> Option<f32> {
        let mut escape_sequence = QuadraticMapSequence::new(test_point, constant_term);

        if convergence_params.refinement_count == 0 {
            if escape_sequence.step_until_condition(
                convergence_params.max_iter_count,
                convergence_params.escape_radius_squared,
            ) {
                return Some(Self::log_iter_count(escape_sequence.iter_count as f32));
            } else {
                return None;
            }
        }

        escape_sequence.compute_normalized_log_escape(
            convergence_params.max_iter_count,
            convergence_params.escape_radius_squared,
            convergence_params.refinement_count,
        )
    }
}

pub fn pixel_renderer<T>(
    image_specification: &ImageSpecification,
    color_map: &ColorMapParams,
    iterative_map: T,
    max_mapped_value: f32,
) -> (
    impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync,
    Histogram,
    CumulativeDistributionFunction,
)
where
    T: Fn(&[f64; 2]) -> Option<f32> + std::marker::Sync,
{
    let background_color = Rgb(color_map.background_color_rgb);

    /////////////////////////////////////////////////////////////////////////

    // Create a reduced-resolution pixel map for the histogram samples:
    let hist_image_spec =
        image_specification.scale_to_total_pixel_count(color_map.histogram_sample_count as i32);

    let mut histogram = Histogram::new(color_map.histogram_bin_count, max_mapped_value);
    let pixel_mapper = PixelMapper::new(&hist_image_spec);

    for i in 0..hist_image_spec.resolution[0] {
        let x = pixel_mapper.width.map(i);
        for j in 0..hist_image_spec.resolution[1] {
            let y = pixel_mapper.height.map(j);
            let maybe_value: Option<f32> = iterative_map(&[x, y]);
            if let Some(value) = maybe_value {
                histogram.insert(value);
            }
        }
    }

    // Now compute the CDF from the histogram, which will allow us to normalize the color distribution
    let cdf = CumulativeDistributionFunction::new(&histogram);

    let base_color_map = ColorMap::new(&color_map.keyframes, LinearInterpolator {});

    let color_map = ColorMapLookUpTable {
        table: LookupTable::new(
            [cdf.min_data, cdf.max_data],
            color_map.lookup_table_count,
            |query: f32| {
                let mapped_query = cdf.percentile(query);
                base_color_map.compute_pixel(mapped_query)
            },
        ),
    };

    (
        move |point: &nalgebra::Vector2<f64>| {
            let maybe_value: Option<f32> = iterative_map(&[point[0], point[1]]);
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

pub trait Renderable: Serialize + std::fmt::Debug + Clone {
    fn renderer(
        self,
    ) -> (
        impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync,
        Histogram,
        CumulativeDistributionFunction,
    );

    fn image_specification(&self) -> &ImageSpecification;
}

pub fn render<T: Renderable>(
    params: T,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch = Stopwatch::new("Render Stopwatch".to_owned());

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        params.image_specification().resolution[0],
        params.image_specification().resolution[1],
    );

    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);

    stopwatch.record_split("basic setup".to_owned());

    let image_specification = params.image_specification().clone();
    let (pixel_renderer, histogram, cdf) = params.renderer();

    stopwatch.record_split("build renderer".to_owned());

    let raw_data = generate_scalar_image(&image_specification, pixel_renderer, Rgb([0, 0, 0]));

    stopwatch.record_split("compute quadratic sequences".to_owned());

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
