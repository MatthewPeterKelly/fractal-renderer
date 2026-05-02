use num::complex::Complex64;
use serde::{Deserialize, Serialize};
use std::{f64::consts::PI, fmt::Debug, sync::Arc};

use crate::{
    core::{
        color_map::{ColorMap, ColorMapLookUpTable, ColorMapper, MultiColorMap},
        file_io::FilePrefix,
        histogram::{CumulativeDistributionFunction, Histogram},
        image_utils::{
            self, ImageSpecification, PixelMapper, RenderOptions, Renderable, SpeedOptimizer,
            scale_down_parameter_for_speed, scale_up_parameter_for_speed,
        },
        interpolation::{ClampedLogInterpolator, LinearInterpolator},
        user_interface,
    },
    fractals::utilities::{populate_histogram, reset_color_map_lookup_table_from_cdf},
};
use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};

// Its often more efficient to compute both the value of a complex function
// and its derivative (slope) at the same time.
pub struct ComplexValueAndSlope {
    value: Complex64,
    slope: Complex64,
}

// A complex-valued function with its derivative (slope).
pub trait ComplexFunctionWithSlope: Serialize + Clone + Debug + Sync {
    fn eval(&self, z: Complex64) -> ComplexValueAndSlope;

    fn newton_step_size(&self) -> f64;

    fn value_divided_by_slope(&self, z: Complex64) -> Complex64 {
        let vs = self.eval(z);
        vs.value / vs.slope
    }

    fn newton_rhapson_step(&self, z: Complex64) -> Complex64 {
        z - self
            .value_divided_by_slope(z)
            .scale(self.newton_step_size())
    }

    /// Returns the index of the root that is closest to `z`.
    fn root_index(&self, z: Complex64) -> usize;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RootsOfUnityParams {
    pub n_roots: i32,
    pub newton_step_size: f64,
}

impl ComplexFunctionWithSlope for RootsOfUnityParams {
    fn eval(&self, z: Complex64) -> ComplexValueAndSlope {
        // f(z) = z^n - 1, f'(z) = n*z^(n-1)
        let z_pow_n_minus_1 = z.powi(self.n_roots - 1);
        ComplexValueAndSlope {
            value: z * z_pow_n_minus_1 - Complex64::new(1.0, 0.0),
            slope: Complex64::new(self.n_roots as f64, 0.0) * z_pow_n_minus_1,
        }
    }

    fn newton_step_size(&self) -> f64 {
        self.newton_step_size
    }

    fn root_index(&self, z: Complex64) -> usize {
        let theta = z.im.atan2(z.re); // Angle in [-π, π]

        // Map angle -> continuous index in [-n/2, n/2], then round.
        // factor = n / (2π) = n * (1 / (2π))
        const INV_TWO_PI: f64 = 0.5 / std::f64::consts::PI;
        let factor = (self.n_roots as f64) * INV_TWO_PI;
        let k = (theta * factor).round() as i32;

        // Wrap to [0, n)
        k.rem_euclid(self.n_roots) as usize
    }
}

/// Parameters / marker type for f(z) = cosh(z) - 1
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoshMinusOneParams {
    /// Scalar multiplier for the Newton step (usually 1.0).
    pub newton_step_size: f64,
}

impl ComplexFunctionWithSlope for CoshMinusOneParams {
    fn eval(&self, z: Complex64) -> ComplexValueAndSlope {
        // f(z)  = cosh(z) - 1
        // f'(z) = sinh(z)
        let value = z.cosh() - Complex64::new(1.0, 0.0);
        let slope = z.sinh();

        ComplexValueAndSlope { value, slope }
    }

    fn newton_step_size(&self) -> f64 {
        self.newton_step_size
    }

    /// Roots of cosh(z) - 1 are at z_k = 2π i k, k ∈ ℤ.
    ///
    ///   1. Project z onto the imaginary axis.
    ///   2. Find the nearest k by rounding Im(z) / (2π).
    ///   3. Map k ∈ ℤ to a usize using a standard bijection:
    ///      k >= 0  →  index = 2k
    ///      k <  0  →  index = -2k - 1
    fn root_index(&self, z: Complex64) -> usize {
        let two_pi = 2.0 * PI;
        let k = (z.im / two_pi).round() as isize;

        if k >= 0 {
            (2 * k) as usize
        } else {
            (-2 * k - 1) as usize
        }
    }
}

pub struct NewtonRhapsonResult {
    /// The point to which the Newton-Rhapson iteration sequence converge.
    pub soln: Complex64,

    /// Number of iterations taken to converge. In range `[0, max_iteration_count]` inclusive.
    pub iteration_count: u32,

    /// A smooth iteration count, used for rendering. It is computed based on the quadratic
    /// convergence behavior of the Newton-Rhapson method near a fixed point.
    pub smooth_iteration_count: f32,
}

/// Returns Some(NewtonRhapsonResult) if the iteration converges within
/// `max_iteration_count` iterations to within `convergence_tolerance`. Otherwise returns None.
pub fn newton_rhapson_iteration_sequence<F: ComplexFunctionWithSlope>(
    system: &F,
    z0: Complex64,
    convergence_tolerance: f64,
    max_iteration_count: u32,
) -> Option<NewtonRhapsonResult> {
    let mut z_prev = z0;
    let mut prev_err: Option<f64> = None;

    for iteration in 0..=max_iteration_count {
        let z_next = system.newton_rhapson_step(z_prev);
        let error = (z_next - z_prev).norm_sqr();

        if error < convergence_tolerance {
            let iteration_count = iteration;
            let smooth_iteration_count = if let Some(e_prev) = prev_err {
                // Guard against the case where error actually hits zero, which would cause ln(0).
                if error > 0.0 {
                    // model error as geometric between e_prev and err
                    let error_ratio = error / e_prev;

                    // Model the error as geometric between the last two steps:
                    //   e_n ≈ e_prev * error_ratio^(n - (k - 1))
                    // Solve e_ν = tol for the fractional iteration index ν:
                    //   ν = (k - 1) + ln(tol / e_prev) / ln(error_ratio)
                    let frac = (convergence_tolerance / e_prev).ln() / error_ratio.ln();
                    ((iteration as f64 - 1.0) + frac) as f32
                } else {
                    iteration as f32
                }
            } else {
                // We converged on the first iteration, so we have no data for interpolation.
                iteration as f32
            };

            return Some(NewtonRhapsonResult {
                soln: z_next,
                iteration_count,
                smooth_iteration_count,
            });
        }

        prev_err = Some(error);
        z_prev = z_next;
    }

    // Only reach here if we fail to converge.
    None
}

/// These parameters are common to all Newton's method fractals, and are not
/// generic over the specific system being solved.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommonParams {
    /// Image dimensions and viewport.
    pub image_specification: ImageSpecification,
    /// Maximum number of Newton-Raphson iterations before giving up.
    pub max_iteration_count: u32,
    /// Tolerance used to detect convergence to a root.
    pub convergence_tolerance: f64,
    /// Rendering options (anti-aliasing, downsampling, etc.).
    pub render_options: RenderOptions,
    /// Per-root gradients plus the cyclic-attractor (non-converged) color.
    pub color: MultiColorMap,
    /// Number of entries in each precomputed color lookup table.
    pub lookup_table_count: usize,
    /// Number of bins in the shared histogram used to normalize gradients.
    pub histogram_bin_count: usize,
    /// Number of samples drawn from the image when populating the histogram.
    pub histogram_sample_count: usize,
}

// The `NewtonsMethodParams` struct encapsulates all parameters needed to
// specify a Newton's method fractal from a JSON file. It uses an enum with
// `Box<dyn>` to allow for different types of systems to be specified.
// It is analgous to `FractalParams`.
#[derive(Serialize, Deserialize, Debug)]
pub struct NewtonsMethodParams {
    pub params: CommonParams,
    pub system: SystemType,
}

// The `NewtonsMethodRenderable` struct encapsulates the parameters and system
// using generics to improve performance of the rendering engine. This is analgous
// to `QuadraticMap`.
pub struct NewtonsMethodRenderable<F: ComplexFunctionWithSlope> {
    pub params: CommonParams,
    pub system: F,
    // Histogram and CDF are shared by all root color maps, and are used to normalize the image.
    pub histogram: Arc<Histogram>,
    pub cdf: CumulativeDistributionFunction,
    // One color map and lookup table per root. The lookup table is generated from the color map
    // and the shared CDF once per render, which speeds up the rendering a bit.
    pub inner_color_maps: Vec<ColorMap<LinearInterpolator>>,
    pub color_maps: Vec<ColorMapLookUpTable>,
}

impl<F: ComplexFunctionWithSlope> NewtonsMethodRenderable<F> {
    pub fn new(params: CommonParams, system: F) -> Self {
        let inner_color_maps: Vec<ColorMap<LinearInterpolator>> = params
            .color
            .color_maps
            .iter()
            .map(|kfs| ColorMap::new(kfs, LinearInterpolator))
            .collect();

        if inner_color_maps.is_empty() {
            panic!("color.color_maps must define at least one color map");
        }

        let color_maps: Vec<ColorMapLookUpTable> = inner_color_maps
            .iter()
            .map(|cm| ColorMapLookUpTable::from_color_map(cm, params.lookup_table_count))
            .collect();

        let histogram = Histogram::new(
            params.histogram_bin_count,
            params.max_iteration_count as f32,
        );

        let mut renderable = Self {
            system,
            cdf: CumulativeDistributionFunction::new(&histogram),
            histogram: histogram.into(),
            color_maps,
            inner_color_maps,
            params,
        };
        renderable.update_color_map();
        renderable
    }

    fn newton_rhapson_iteration_sequence(&self, z0: Complex64) -> Option<NewtonRhapsonResult> {
        newton_rhapson_iteration_sequence(
            &self.system,
            z0,
            self.params.convergence_tolerance,
            self.params.max_iteration_count,
        )
    }

    fn update_color_map(&mut self) {
        // This histogram uses data shared from all roots, so we do not need the `_soln` value in the below
        // closure. Then we update all color maps based on the shared CDF, which is generated from the histogram.
        populate_histogram(
            &|point: &[f64; 2]| {
                self.newton_rhapson_iteration_sequence(Complex64::new(point[0], point[1]))
                    .map(|result| result.iteration_count as f32)
            },
            &self.params.image_specification,
            self.params.histogram_bin_count as u32,
            self.histogram.clone(),
        );
        self.cdf.reset(&self.histogram);

        for (color_table, inner_map) in self.color_maps.iter_mut().zip(self.inner_color_maps.iter())
        {
            reset_color_map_lookup_table_from_cdf(color_table, &self.cdf, inner_map);
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SystemType {
    RootsOfUnity(Box<RootsOfUnityParams>), // f(z) = z^n - 1
    CoshMinusOne(Box<CoshMinusOneParams>), // f(z) cosh(z) - 1
}

impl<F> SpeedOptimizer for NewtonsMethodRenderable<F>
where
    F: ComplexFunctionWithSlope,
{
    type ReferenceCache = CommonParams;
    fn reference_cache(&self) -> CommonParams {
        self.params.clone()
    }

    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache) {
        self.params
            .render_options
            .set_speed_optimization_level(level, &cache.render_options);

        self.params.max_iteration_count = scale_down_parameter_for_speed(
            32.0,
            cache.max_iteration_count as f64,
            level,
            ClampedLogInterpolator,
        ) as u32;

        self.params.convergence_tolerance = scale_up_parameter_for_speed(
            0.005,
            cache.convergence_tolerance,
            level,
            ClampedLogInterpolator,
        );

        self.params.histogram_sample_count = scale_down_parameter_for_speed(
            600.0,
            cache.histogram_sample_count as f64,
            level,
            ClampedLogInterpolator,
        ) as usize;
    }
}

impl<F> Renderable for NewtonsMethodRenderable<F>
where
    F: ComplexFunctionWithSlope + Sync + Send,
{
    type Params = CommonParams;
    type ColorMap = MultiColorMap;

    fn image_specification(&self) -> &ImageSpecification {
        &self.params.image_specification
    }

    fn render_options(&self) -> &RenderOptions {
        &self.params.render_options
    }

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        self.params.image_specification = image_specification;
        self.update_color_map();
    }

    fn render_point(&self, point: &[f64; 2]) -> image::Rgb<u8> {
        let result =
            match self.newton_rhapson_iteration_sequence(Complex64::new(point[0], point[1])) {
                Some(res) => res,
                None => {
                    return image::Rgb(self.params.color.cyclic_attractor);
                }
            };

        // Use the solution to select the correct color map for this point:
        let color_map_index = self.system.root_index(result.soln) % self.color_maps.len();
        self.color_maps[color_map_index].compute_pixel(result.smooth_iteration_count)
    }

    fn write_diagnostics<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.histogram.display(writer)?;
        self.cdf.display(writer)?;
        std::io::Result::Ok(())
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }

    fn color_map(&self) -> &Self::ColorMap {
        &self.params.color
    }

    fn compute_raw_field(&self, sampling_level: i32, field: &mut Vec<Vec<Option<(f32, u32)>>>) {
        debug_assert!(
            sampling_level >= 0,
            "Phase 2.1 supports only sampling_level >= 0"
        );
        let n = (sampling_level.max(0) + 1) as usize;
        let spec = &self.params.image_specification;
        let pixel_map = PixelMapper::new(spec);
        let pixel_width = spec.width / spec.resolution[0] as f64;
        let pixel_height = spec.height() / spec.resolution[1] as f64;
        let step = 1.0 / n as f64;
        let n_color_maps = self.color_maps.len() as u32;

        field.par_iter_mut().enumerate().for_each(|(idx, col)| {
            let px = (idx / n) as u32;
            let i = idx % n;
            let re = pixel_map.width.map(px) + (i as f64) * step * pixel_width;
            for (idy, cell) in col.iter_mut().enumerate() {
                let py = (idy / n) as u32;
                let j = idy % n;
                let im = pixel_map.height.map(py) + (j as f64) * step * pixel_height;
                *cell = self
                    .newton_rhapson_iteration_sequence(Complex64::new(re, im))
                    .map(|res| {
                        let k = (self.system.root_index(res.soln) as u32) % n_color_maps.max(1);
                        (res.smooth_iteration_count, k)
                    });
            }
        });
    }

    fn populate_histogram(
        &self,
        _sampling_level: i32,
        _field: &[Vec<Option<(f32, u32)>>],
        histogram: &Histogram,
    ) {
        // Phase 2.1: continue sampling on a sub-sample grid (matches the
        // legacy histogram source); 2.3 switches this to a full-field walk.
        let sample_count = self.params.histogram_sample_count as u32;
        let hist_image_spec = self
            .params
            .image_specification
            .scale_to_total_pixel_count(sample_count);
        let pixel_mapper = PixelMapper::new(&hist_image_spec);
        use rayon::iter::IntoParallelIterator;
        (0..hist_image_spec.resolution[0])
            .into_par_iter()
            .for_each(|i| {
                let x = pixel_mapper.width.map(i);
                for j in 0..hist_image_spec.resolution[1] {
                    let y = pixel_mapper.height.map(j);
                    if let Some(result) =
                        self.newton_rhapson_iteration_sequence(Complex64::new(x, y))
                    {
                        // Legacy histogram source: integer iteration_count.
                        // 2.3 switches this to a full-field walk over smooth values.
                        histogram.insert(result.iteration_count as f32);
                    }
                }
            });
    }

    fn normalize_field(
        &self,
        _sampling_level: i32,
        cdf: &CumulativeDistributionFunction,
        field: &mut Vec<Vec<Option<(f32, u32)>>>,
    ) {
        field.par_iter_mut().for_each(|col| {
            for (s, _k) in col.iter_mut().flatten() {
                *s = cdf.percentile(*s);
            }
        });
    }
}

// Renders a Newton's method fractal based on the provided parameters.
pub fn render_newtons_method(
    params: &NewtonsMethodParams,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    // This indirection step is important for performance -- here we unwrap all of the `dyn` pointers
    // and implement all of the inner render loops using generics for performance.

    match &params.system {
        SystemType::RootsOfUnity(system_params) => image_utils::render(
            NewtonsMethodRenderable::new(params.params.clone(), system_params.as_ref().clone()),
            file_prefix,
        ),
        SystemType::CoshMinusOne(system_params) => image_utils::render(
            NewtonsMethodRenderable::new(params.params.clone(), system_params.as_ref().clone()),
            file_prefix,
        ),
    }
}

pub fn explore_fractal(
    params: &NewtonsMethodParams,
    mut file_prefix: FilePrefix,
) -> eframe::Result<()> {
    match &params.system {
        SystemType::RootsOfUnity(system_params) => {
            file_prefix.create_and_step_into_sub_directory("roots_of_unity");
            user_interface::explore(
                file_prefix,
                params.params.image_specification,
                NewtonsMethodRenderable::new(params.params.clone(), system_params.as_ref().clone()),
            )
        }
        SystemType::CoshMinusOne(system_params) => {
            file_prefix.create_and_step_into_sub_directory("cosh_minus_one");
            user_interface::explore(
                file_prefix,
                params.params.image_specification,
                NewtonsMethodRenderable::new(params.params.clone(), system_params.as_ref().clone()),
            )
        }
    }
}
