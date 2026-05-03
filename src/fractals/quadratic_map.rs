use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefMutIterator, ParallelIterator,
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::core::{
    color_map::BackgroundWithColorMap,
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::{
        ImageSpecification, PixelMapper, RenderOptions, Renderable, SpeedOptimizer,
        scale_down_parameter_for_speed,
    },
    interpolation::{ClampedLinearInterpolator, ClampedLogInterpolator},
};

/// Parameter block for the colorization step of escape-time fractals
/// (Mandelbrot, Julia). The `color` field holds the user-facing palette;
/// the remaining fields tune the histogram-based normalization and the
/// pre-baked lookup table.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorMapParams {
    /// Background color and gradient keyframes for escaped pixels.
    pub color: BackgroundWithColorMap,
    /// Number of entries in the precomputed color lookup table.
    pub lookup_table_count: usize,
    /// Number of bins used by the histogram that drives gradient normalization.
    pub histogram_bin_count: usize,
    /// Number of samples drawn from the image when populating the histogram.
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
    fn convergence_params_mut(&mut self) -> &mut ConvergenceParams;

    /// Access the color map parameters.
    fn color_map_params(&self) -> &ColorMapParams;
    /// Mutable access to the color map parameters.
    fn color_map_params_mut(&mut self) -> &mut ColorMapParams;

    /// Access to the rendering options:
    fn render_options(&self) -> &RenderOptions;
    fn render_options_mut(&mut self) -> &mut RenderOptions;

    // Actually evaluate the fractal.
    fn normalized_log_escape_count(&self, point: &[f64; 2]) -> Option<f32>;
}

/// Reference cache used by `SpeedOptimizer` to interpolate runtime
/// parameters back toward the user's specified values.
pub struct ParamsReferenceCache {
    /// User-specified `histogram_sample_count`.
    pub histogram_sample_count: usize,
    /// User-specified `max_iter_count`.
    pub max_iter_count: u32,
    /// User-specified render options (including `sampling_level`).
    pub render_options: RenderOptions,
}

/// Newtype wrapper that carries the fractal parameters and implements
/// `Renderable` for the rendering pipeline. Histogram, CDF, and color
/// caches now live in the pipeline, not here.
pub struct QuadraticMap<T: QuadraticMapParams> {
    /// User-facing parameters that drive convergence, color, and rendering.
    pub fractal_params: T,
}

impl<T: QuadraticMapParams> QuadraticMap<T> {
    /// Construct a `QuadraticMap` from its parameters. Allocation-free
    /// after this call (the pipeline owns the actual buffers).
    pub fn new(fractal_params: T) -> QuadraticMap<T> {
        QuadraticMap { fractal_params }
    }
}

impl<T> SpeedOptimizer for QuadraticMap<T>
where
    T: QuadraticMapParams,
{
    type ReferenceCache = ParamsReferenceCache;

    fn reference_cache(&self) -> Self::ReferenceCache {
        ParamsReferenceCache {
            histogram_sample_count: self
                .fractal_params
                .color_map_params()
                .histogram_sample_count,
            max_iter_count: self.fractal_params.convergence_params().max_iter_count,
            render_options: *self.fractal_params.render_options(),
        }
    }

    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache) {
        self.fractal_params
            .color_map_params_mut()
            .histogram_sample_count = scale_down_parameter_for_speed(
            1024.0,
            cache.histogram_sample_count as f64,
            level,
            ClampedLogInterpolator,
        ) as usize;

        self.fractal_params.convergence_params_mut().max_iter_count = scale_down_parameter_for_speed(
            128.0,
            cache.max_iter_count as f64,
            level,
            ClampedLinearInterpolator,
        ) as u32;
        self.fractal_params
            .render_options_mut()
            .set_speed_optimization_level(level, &cache.render_options);
    }
}

impl<T> Renderable for QuadraticMap<T>
where
    T: QuadraticMapParams + Sync + Send,
{
    type Params = T;
    type ColorMap = BackgroundWithColorMap;

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        self.fractal_params
            .set_image_specification(image_specification);
    }

    fn write_diagnostics<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Ok(())
    }

    fn params(&self) -> &Self::Params {
        &self.fractal_params
    }

    fn image_specification(&self) -> &ImageSpecification {
        self.fractal_params.image_specification()
    }

    fn render_options(&self) -> &RenderOptions {
        self.fractal_params.render_options()
    }

    fn color_map(&self) -> &Self::ColorMap {
        &self.fractal_params.color_map_params().color
    }

    fn histogram_bin_count(&self) -> usize {
        self.fractal_params.color_map_params().histogram_bin_count
    }

    fn histogram_max_value(&self) -> f32 {
        QuadraticMapSequence::log_iter_count(
            self.fractal_params.convergence_params().max_iter_count as f32,
        )
    }

    fn lookup_table_count(&self) -> usize {
        self.fractal_params.color_map_params().lookup_table_count
    }

    fn compute_raw_field(&self, sampling_level: i32, field: &mut Vec<Vec<Option<f32>>>) {
        let spec = *self.fractal_params.image_specification();
        let n_max_plus_1 = field.len() / spec.resolution[0] as usize;
        compute_raw_field_quadratic(&spec, n_max_plus_1, sampling_level, field, |p| {
            self.fractal_params.normalized_log_escape_count(p)
        });
    }

    fn populate_histogram(
        &self,
        _sampling_level: i32,
        _field: &[Vec<Option<f32>>],
        histogram: &Histogram,
    ) {
        // Phase 2.2 keeps the legacy sub-sample-grid histogram source so
        // that pixel hashes track previous behavior; Phase 2.3 switches
        // this to a full-field walk over the populated cells.
        let sample_count = self
            .fractal_params
            .color_map_params()
            .histogram_sample_count as u32;
        let hist_image_spec = self
            .fractal_params
            .image_specification()
            .scale_to_total_pixel_count(sample_count);
        let pixel_mapper = PixelMapper::new(&hist_image_spec);
        (0..hist_image_spec.resolution[0])
            .into_par_iter()
            .for_each(|i| {
                let x = pixel_mapper.width.map(i);
                for j in 0..hist_image_spec.resolution[1] {
                    let y = pixel_mapper.height.map(j);
                    if let Some(value) = self.fractal_params.normalized_log_escape_count(&[x, y]) {
                        histogram.insert(value);
                    }
                }
            });
    }

    fn normalize_field(
        &self,
        sampling_level: i32,
        cdf: &CumulativeDistributionFunction,
        field: &mut Vec<Vec<Option<f32>>>,
    ) {
        let spec = *self.fractal_params.image_specification();
        let n_max_plus_1 = field.len() / spec.resolution[0] as usize;
        normalize_populated_cells(sampling_level, n_max_plus_1, field, |v| cdf.percentile(*v));
    }
}

/// Populate the cells of `field` reachable at the requested `sampling_level`.
///
/// - **Positive `sampling_level = r`**: each output pixel block (`n_max_plus_1²`
///   cells) gets the first `(r+1)²` cells populated, evaluated at subpixel
///   positions `(i / (r+1), j / (r+1))` of the pixel for `i, j ∈ 0..(r+1)`.
///   At `r == n_max_plus_1 - 1` this fills the whole field; at `r == 0` only
///   one cell per block is touched.
/// - **`sampling_level == 0`**: same as above with `r = 0` (one cell per
///   block).
/// - **Negative `sampling_level = -m`**: block-fill. Each `(m+1) × (m+1)`
///   output-pixel block uses one shared evaluation, stored at the top-left
///   field cell of the leftmost output pixel of the block.
fn compute_raw_field_quadratic<F>(
    image_specification: &ImageSpecification,
    n_max_plus_1: usize,
    sampling_level: i32,
    field: &mut Vec<Vec<Option<f32>>>,
    escape_count: F,
) where
    F: Fn(&[f64; 2]) -> Option<f32> + Sync,
{
    let pixel_map = PixelMapper::new(image_specification);
    let pixel_width = image_specification.width / image_specification.resolution[0] as f64;
    let pixel_height = image_specification.height() / image_specification.resolution[1] as f64;

    if sampling_level >= 0 {
        let n = sampling_level as usize + 1;
        let step = 1.0 / n as f64;
        field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
            let i = outer_x % n_max_plus_1;
            if i >= n {
                return;
            }
            let px = (outer_x / n_max_plus_1) as u32;
            let re = pixel_map.width.map(px) + (i as f64) * step * pixel_width;
            for (outer_y, cell) in col.iter_mut().enumerate() {
                let j = outer_y % n_max_plus_1;
                if j >= n {
                    continue;
                }
                let py = (outer_y / n_max_plus_1) as u32;
                let im = pixel_map.height.map(py) + (j as f64) * step * pixel_height;
                *cell = escape_count(&[re, im]);
            }
        });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        let stride = n_max_plus_1 * block_size;
        field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
            if outer_x % stride != 0 {
                return;
            }
            let block_x = outer_x / stride;
            let px = (block_x * block_size) as u32;
            let re = pixel_map.width.map(px);
            for (outer_y, cell) in col.iter_mut().enumerate() {
                if outer_y % stride != 0 {
                    continue;
                }
                let block_y = outer_y / stride;
                let py = (block_y * block_size) as u32;
                let im = pixel_map.height.map(py);
                *cell = escape_count(&[re, im]);
            }
        });
    }
}

/// Walk the same set of populated cells `compute_raw_field_quadratic`
/// fills, applying `f` to each `Some` value in place. Used by
/// `normalize_field` to rewrite raw escape counts as CDF percentiles.
fn normalize_populated_cells<F: Fn(&f32) -> f32 + Sync>(
    sampling_level: i32,
    n_max_plus_1: usize,
    field: &mut Vec<Vec<Option<f32>>>,
    f: F,
) {
    if sampling_level >= 0 {
        let n = sampling_level as usize + 1;
        field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
            let i = outer_x % n_max_plus_1;
            if i >= n {
                return;
            }
            for (outer_y, cell) in col.iter_mut().enumerate() {
                let j = outer_y % n_max_plus_1;
                if j >= n {
                    continue;
                }
                if let Some(v) = cell {
                    *v = f(v);
                }
            }
        });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        let stride = n_max_plus_1 * block_size;
        field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
            if outer_x % stride != 0 {
                return;
            }
            for (outer_y, cell) in col.iter_mut().enumerate() {
                if outer_y % stride != 0 {
                    continue;
                }
                if let Some(v) = cell {
                    *v = f(v);
                }
            }
        });
    }
}
