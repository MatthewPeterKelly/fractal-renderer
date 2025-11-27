use num::complex::Complex64;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, sync::Arc};

use crate::core::{
    file_io::{serialize_to_json_or_panic, FilePrefix},
    image_utils::{self, ImageSpecification, RenderOptions, Renderable, SpeedOptimizer},
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

// Its often more efficient to compute both the value of a complex function
// and its derivative (slope) at the same time.
pub struct ComplexValueAndSlope {
    value: Complex64,
    slope: Complex64,
}

// A complex-valued function with its derivative (slope).
// MPK: analgous to `QuadraticMapParams`
pub trait ComplexFunctionWithSlope: Serialize + Clone + Debug + Sync {
    fn eval(&self, z: Complex64) -> ComplexValueAndSlope;

    fn value_divided_by_slope(&self, z: Complex64) -> Complex64 {
        let vs = self.eval(z);
        vs.value / vs.slope
    }

    fn newton_rhapson_step(&self, z: Complex64, alpha: f64) -> Complex64 {
        z - self.value_divided_by_slope(z).scale(alpha)
    }
}

// Function that runs a complete "Newton's method" iteration seuqence until
// convergence or max iterations.
pub fn newton_rhapson_iteration_sequence<F: ComplexFunctionWithSlope>(
    system: &F,
    z0: Complex64,
    convergence_tolerance: f64,
    iteration_limits: [u32; 2],
    alpha: f64,
) -> (Complex64, u32) {
    let mut z = z0;
    for _ in 0..iteration_limits[0] {
        z = system.newton_rhapson_step(z, alpha);
    }
    for iteration in iteration_limits[0]..iteration_limits[1] {
        let z_next = system.newton_rhapson_step(z, alpha);
        if (z_next - z).norm_sqr() < convergence_tolerance {
            return (z_next, iteration + 1);
        }
        z = z_next;
    }
    (z, iteration_limits[1])
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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SystemType {
    RootsOfUnity(Box<RootsOfUnityParams>), // number of roots == root_colors_rgb.len()
}

// MPK:  architecture: analgous to `QuadraticMapParams`
#[derive(Serialize, Deserialize, Debug, Clone)]
struct RootsOfUnityParams {
    pub n_roots: i32,
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
}

// MPK:  analogous to QuadraticMap
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommonParams {
    pub image_specification: ImageSpecification,
    pub iteration_limits: [u32; 2], // [min, max]
    pub convergence_tolerance: f64,
    pub render_options: RenderOptions,
    pub background_color_rgb: [u8; 3],
    pub cyclic_attractor_color_rgb: [u8; 3],
    pub root_colors_rgb: Vec<[u8; 3]>,
    pub grayscale_keyframes: Vec<GrayscaleMapKeyFrame>,
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

    fn render_point(&self, point: &[f64; 2]) -> image::Rgb<u8> {
        todo!()
    }

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        self.params.image_specification = image_specification;
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
            NewtonsMethodRenderable {
                params: params.params.clone(),
                system: system_params.as_ref().clone(),
            },
            file_prefix,
        ),
    }
}
