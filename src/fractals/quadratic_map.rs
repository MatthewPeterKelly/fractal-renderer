use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::core::{
    color_map::ColorMap,
    field_iteration::FieldKernel,
    image_utils::{
        ImageSpecification, RenderOptions, Renderable, SpeedOptimizer,
        scale_down_parameter_for_speed,
    },
    interpolation::ClampedLinearInterpolator,
};

/// Parameter block for the colorization step of escape-time fractals
/// (Mandelbrot, Julia). The `color` field holds the user-facing palette;
/// the remaining fields tune the histogram-based normalization and the
/// pre-baked lookup table.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorMapParams {
    /// Flat color (for in-set pixels) and one gradient (for escaped pixels).
    pub color: ColorMap,
    /// Number of entries in the precomputed color lookup table.
    pub lookup_table_count: usize,
    /// Number of bins used by the histogram that drives gradient normalization.
    pub histogram_bin_count: usize,
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

/// Trait implemented by Mandelbrot and Julia parameter types. Each
/// implementation supplies the actual escape-count math; the
/// `Renderable` / `FieldKernel` impls live as blanket impls below so
/// the per-fractal types stay parameter-only.
pub trait QuadraticMapParams: Serialize + Clone + Debug + Sync + Send {
    /// Access the current image specification.
    fn image_specification(&self) -> &ImageSpecification;

    /// Update the image specification.
    fn set_image_specification(&mut self, image_specification: ImageSpecification);

    /// Access the convergence parameters.
    fn convergence_params(&self) -> &ConvergenceParams;
    fn convergence_params_mut(&mut self) -> &mut ConvergenceParams;

    /// Access the color map parameters.
    fn color_map_params(&self) -> &ColorMapParams;
    /// Mutable access to the color map parameters. Used by the live editor
    /// flow (Phase 7) to mutate keyframes through the renderer.
    #[allow(dead_code)]
    fn color_map_params_mut(&mut self) -> &mut ColorMapParams;

    /// Access to the rendering options:
    fn render_options(&self) -> &RenderOptions;
    fn render_options_mut(&mut self) -> &mut RenderOptions;

    /// Evaluate the smooth log-escape count at the given point.
    fn normalized_log_escape_count(&self, point: &[f64; 2]) -> Option<f32>;
}

/// Reference cache used by `SpeedOptimizer` to interpolate runtime
/// parameters back toward the user's specified values.
pub struct ParamsReferenceCache {
    /// User-specified `max_iter_count`.
    pub max_iter_count: u32,
    /// User-specified render options (including `sampling_level`).
    pub render_options: RenderOptions,
}

impl<T: QuadraticMapParams> SpeedOptimizer for T {
    type ReferenceCache = ParamsReferenceCache;

    fn reference_cache(&self) -> Self::ReferenceCache {
        ParamsReferenceCache {
            max_iter_count: self.convergence_params().max_iter_count,
            render_options: *self.render_options(),
        }
    }

    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache) {
        self.convergence_params_mut().max_iter_count = scale_down_parameter_for_speed(
            128.0,
            cache.max_iter_count as f64,
            level,
            ClampedLinearInterpolator,
        ) as u32;
        self.render_options_mut()
            .set_speed_optimization_level(level, &cache.render_options);
    }
}

impl<T: QuadraticMapParams> FieldKernel for T {
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
        self.normalized_log_escape_count(&point).map(|v| (v, 0))
    }
}

impl<T: QuadraticMapParams> Renderable for T {
    type Params = T;

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        QuadraticMapParams::set_image_specification(self, image_specification);
    }

    fn write_diagnostics<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Ok(())
    }

    fn params(&self) -> &Self::Params {
        self
    }

    fn image_specification(&self) -> &ImageSpecification {
        QuadraticMapParams::image_specification(self)
    }

    fn render_options(&self) -> &RenderOptions {
        QuadraticMapParams::render_options(self)
    }

    fn color_map(&self) -> &ColorMap {
        &self.color_map_params().color
    }

    fn color_map_mut(&mut self) -> &mut ColorMap {
        &mut self.color_map_params_mut().color
    }

    fn histogram_bin_count(&self) -> usize {
        self.color_map_params().histogram_bin_count
    }

    fn histogram_max_value(&self) -> f32 {
        QuadraticMapSequence::log_iter_count(self.convergence_params().max_iter_count as f32)
    }

    fn lookup_table_count(&self) -> usize {
        self.color_map_params().lookup_table_count
    }
}
