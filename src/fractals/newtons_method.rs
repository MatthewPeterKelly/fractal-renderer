use num::complex::Complex64;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, sync::Arc};

use crate::core::{
    color_map::{ColorMap, ColorMapKeyFrame, ColorMapper},
    file_io::FilePrefix,
    image_utils::{self, ImageSpecification, RenderOptions, Renderable, SpeedOptimizer},
    interpolation::LinearInterpolator,
};

// Used to interpolate between two color values based on the iterations
// required for the Newton-Raphson method to converge to a root.
// Query values of 0 map to `iteration_limits[0]` and values of 1 map to
// `iteration_limits[1]`. The `value` of zero corresponds to the common
// background color, while a `value` of one corresponds to the foreground
// color associated with the root that the iteration converges to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct GrayscaleMapKeyFrame {
    pub query: f32,
    pub value: f32,
}

// MPK:  analogous to QuadraticMap
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommonParams {
    pub image_specification: ImageSpecification,
    pub iteration_limits: [u32; 2], // [min, max]
    pub convergence_tolerance: f64,
    pub render_options: RenderOptions,
    pub background_color_rgb: [u8; 3],
    pub cyclic_attractor_color_rgb: [u8; 3], // did not converge
    pub root_colors_rgb: Vec<[u8; 3]>,
    pub grayscale_keyframes: Vec<GrayscaleMapKeyFrame>,
    pub lookup_table_count: usize,
    pub histogram_bin_count: usize,
    pub histogram_sample_count: usize,
}

// Its often more efficient to compute both the value of a complex function
// and its derivative (slope) at the same time.
pub struct ComplexValueAndSlope {
    value: Complex64,
    slope: Complex64,
}

// A complex-valued function with its derivative (slope).
// MPK: analgous to `QuadraticMapParams`
// TODO: consider renaming this!
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

// Function that runs a complete "Newton's method" iteration seuqence until
// convergence or max iterations. Note that iteration limits are inclusive,
// and that an "reached iteration limit" will return an index larger than the
// upper iteration limit
pub fn newton_rhapson_iteration_sequence<F: ComplexFunctionWithSlope>(
    system: &F,
    z0: Complex64,
    convergence_tolerance: f64,
    iteration_limits: [u32; 2], // inclusive
) -> (Complex64, u32) {
    let mut z = z0;
    for _ in 0..iteration_limits[0] {
        z = system.newton_rhapson_step(z);
    }
    for iteration in iteration_limits[0]..iteration_limits[1] {
        let z_next = system.newton_rhapson_step(z);
        if (z_next - z).norm_sqr() < convergence_tolerance {
            return (z_next, iteration + 1);
        }
        z = z_next;
    }
    (z, iteration_limits[1] + 1)
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
    pub color_maps: Vec<ColorMap<LinearInterpolator>>,
}

impl<F: ComplexFunctionWithSlope> NewtonsMethodRenderable<F> {
    pub fn new(params: CommonParams, system: F) -> Self {
        // TODO: consider alternate forumulation here...
        let mut inner_color_map = Vec::new();
        for root_color in &params.root_colors_rgb {
            let keyframes: Vec<ColorMapKeyFrame> = params
                .grayscale_keyframes
                .iter()
                .map(|gkf| {
                    let t = gkf.value.clamp(0.0, 1.0);

                    let mut rgb = [0u8; 3];
                    for i in 0..3 {
                        let bg = params.background_color_rgb[i] as f32;
                        let root = root_color[i] as f32;
                        let v = bg + (root - bg) * t;
                        rgb[i] = v.clamp(0.0, 255.0).round() as u8;
                    }

                    ColorMapKeyFrame {
                        query: gkf.query,
                        rgb_raw: rgb,
                    }
                })
                .collect();

            let color_map = ColorMap::new(&keyframes, LinearInterpolator);
            inner_color_map.push(color_map);
        }
        Self {
            params,
            system,
            color_maps: inner_color_map,
        }
    }

    fn update_color_map(&mut self) {
        // TODO:  eventually.
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SystemType {
    RootsOfUnity(Box<RootsOfUnityParams>), // number of roots == root_colors_rgb.len()
}

// MPK:  architecture: analgous to `QuadraticMapParams`
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

impl<F> SpeedOptimizer for NewtonsMethodRenderable<F>
where
    F: ComplexFunctionWithSlope,
{
    type ReferenceCache = CommonParams;
    fn reference_cache(&self) -> CommonParams {
        self.params.clone()
    }

    fn set_speed_optimization_level(&mut self, _level: f64, _cache: &Self::ReferenceCache) {
        // Skip this for now -- easy enough to drop in later.
        // TODO:  implement this so that explore mode works nicely.
    }
}

impl<F> Renderable for NewtonsMethodRenderable<F>
where
    F: ComplexFunctionWithSlope + Sync + Send,
{
    type Params = CommonParams;
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
        let (soln, iter) = newton_rhapson_iteration_sequence(
            &self.system,
            Complex64::new(point[0], point[1]),
            self.params.convergence_tolerance,
            self.params.iteration_limits,
        );

        if iter > self.params.iteration_limits[1] {
            return image::Rgb(self.params.cyclic_attractor_color_rgb);
        }
        // ToDo:  eventually wew could be fancy and do quadratic interpolation here
        let scaled_iteration_count = (iter - self.params.iteration_limits[0]) as f64
            / (self.params.iteration_limits[1] - self.params.iteration_limits[0]) as f64;

        let color_map_index = self.system.root_index(soln) % self.color_maps.len();
        self.color_maps[color_map_index].compute_pixel(scaled_iteration_count as f32)
    }

    fn write_diagnostics<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Eventually we could add more here...
        std::io::Result::Ok(())
    }

    fn params(&self) -> &Self::Params {
        &self.params
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
    }
}
