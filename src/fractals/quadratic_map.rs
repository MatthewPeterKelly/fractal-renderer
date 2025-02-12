use image::Rgb;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::core::{
    color_map::{ColorMap, ColorMapKeyFrame, ColorMapLookUpTable, ColorMapper, LinearInterpolator},
    file_io::{serialize_to_json_or_panic, FilePrefix},
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::{
        generate_scalar_image, write_image_to_file_or_panic, ImageSpecification, PixelMapper,
        Renderable,
    },
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

pub trait QuadraticMapParams: Serialize + Clone + Debug + Sync {
    /// Access the current image specification.
    fn image_specification(&self) -> &ImageSpecification;

    /// Update the image specification.
    fn set_image_specification(&mut self, image_specification: ImageSpecification);

    /// Access the convergence parameters.
    fn convergence_params(&self) -> &ConvergenceParams;

    /// Access the color map parameters.
    fn color_map(&self) -> &ColorMapParams;

    // Actually evaluate the fractal.
    fn normalized_log_escape_count(&self, point: &[f64; 2]) -> Option<f32>;
}

pub fn populate_histogram<T: QuadraticMapParams>(fractal_params: &T, histogram: &mut Histogram) {
    let hist_image_spec = fractal_params
        .image_specification()
        .scale_to_total_pixel_count(fractal_params.color_map().histogram_sample_count as i32);

    let pixel_mapper = PixelMapper::new(&hist_image_spec);

    (0..hist_image_spec.resolution[0])
        .into_par_iter()
        .for_each(|i| {
            let x = pixel_mapper.width.map(i);
            for j in 0..hist_image_spec.resolution[1] {
                let y = pixel_mapper.height.map(j);
                if let Some(value) = fractal_params.normalized_log_escape_count(&[x, y]) {
                    histogram.insert(value);
                }
            }
        });
}

pub fn create_empty_histogram<T: QuadraticMapParams>(params: &T) -> Histogram {
    Histogram::new(
        params.color_map().histogram_bin_count,
        QuadraticMapSequence::log_iter_count(params.convergence_params().max_iter_count as f32),
    )
}

pub struct QuadraticMap<T: QuadraticMapParams> {
    pub fractal_params: T,
    pub histogram: Histogram,
    pub cdf: CumulativeDistributionFunction,
    pub color_map: ColorMapLookUpTable,
    pub inner_color_map: ColorMap<LinearInterpolator>,
    pub background_color: Rgb<u8>,
}

impl<T: QuadraticMapParams> QuadraticMap<T> {
    pub fn new(fractal_params: T) -> QuadraticMap<T> {
        let inner_color_map =
            ColorMap::new(&fractal_params.color_map().keyframes, LinearInterpolator {});
        let mut quadratic_map = QuadraticMap {
            fractal_params: fractal_params.clone(),
            histogram: Histogram::default(),
            cdf: CumulativeDistributionFunction::default(),
            color_map: ColorMapLookUpTable::from_color_map(
                &inner_color_map,
                fractal_params.color_map().lookup_table_count,
            ),
            inner_color_map,
            background_color: Rgb(fractal_params.color_map().background_color_rgb),
        };
        quadratic_map.histogram = create_empty_histogram(&quadratic_map.fractal_params);
        quadratic_map.cdf = CumulativeDistributionFunction::new(&quadratic_map.histogram);
        quadratic_map.update_color_map();
        quadratic_map
    }

    fn update_color_map(&mut self) {
        self.histogram.reset();
        populate_histogram(&self.fractal_params, &mut self.histogram);

        self.cdf.reset(&self.histogram);

        // Aliases to let the borrow checker verify that we're all good here.
        let cdf_ref = &self.cdf;
        let inner_map_ref = &self.inner_color_map;

        self.color_map
            .reset([cdf_ref.min_data, cdf_ref.max_data], &|query: f32| {
                let mapped_query = cdf_ref.percentile(query);
                inner_map_ref.compute_pixel(mapped_query)
            });
    }
}

impl<T> Renderable for QuadraticMap<T>
where
    T: QuadraticMapParams + Sync + Send,
{
    type Params = T;

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        self.fractal_params
            .set_image_specification(image_specification);
        self.update_color_map();
    }

    fn write_diagnostics<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.histogram.display(writer)?;
        self.cdf.display(writer)?;
        std::io::Result::Ok(())
    }

    fn params(&self) -> &Self::Params {
        &self.fractal_params
    }

    fn render_point(&self, point: &nalgebra::Vector2<f64>) -> Rgb<u8> {
        let maybe_escape_count = self
            .fractal_params
            .normalized_log_escape_count(&[point[0], point[1]]);
        if let Some(value) = maybe_escape_count {
            self.color_map.compute_pixel(value)
        } else {
            self.background_color
        }
    }

    fn image_specification(&self) -> &ImageSpecification {
        self.fractal_params.image_specification()
    }
}

pub fn render<T: Renderable>(
    renderable: T,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch = Stopwatch::new("Render Stopwatch".to_owned());

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        renderable.image_specification().resolution[0],
        renderable.image_specification().resolution[1],
    );

    serialize_to_json_or_panic(
        file_prefix.full_path_with_suffix(".json"),
        renderable.params(),
    );

    stopwatch.record_split("basic setup".to_owned());

    let image_specification = renderable.image_specification().clone();
    let pixel_renderer = |point: &nalgebra::Vector2<f64>| renderable.render_point(point);
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
    renderable.write_diagnostics(&mut diagnostics_file)?;

    Ok(())
}
