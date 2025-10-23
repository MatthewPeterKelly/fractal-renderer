// Common library funcions for fractals that are backed by Newton's method,
// such as the roots of unity fractal.

#[cfg(test)]
use nalgebra::Matrix2;
use num::complex::Complex64;

use serde::{Deserialize, Serialize};

use crate::core::{
    file_io::{serialize_to_json_or_panic, FilePrefix},
    image_utils::{ImageSpecification, RenderOptions},
};

/// Used to interpolate between two color values based on the iterations
/// required for the Newton-Raphson method to converge to a root.
/// Query values of 0 map to `iteration_limits[0]` and values of 1 map to
/// `iteration_limits[1]`. The `value` of zero corresponds to the common
/// background color, while a `value` of one corresponds to the foreground
/// color associated with the root that the iteration converges to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct GrayscaleMapKeyFrame {
    pub query: f32,
    pub value: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ComplexFunctionType {
    RootsOfUnity, // number of roots == root_colors_rgb.len()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewtonsMethodFractalParams {
    pub function_type: ComplexFunctionType,
    pub image_specification: ImageSpecification,
    pub iteration_limits: [u32; 2], // [min, max]
    pub convergence_tolerance: f64,
    pub render_options: RenderOptions,
    pub background_color_rgb: [u8; 3],
    pub cyclic_attractor_color_rgb: [u8; 3],
    pub root_colors_rgb: Vec<[u8; 3]>,
    pub grayscale_keyframes: Vec<GrayscaleMapKeyFrame>,
}

pub struct ComplexValueAndSlope {
    value: Complex64,
    slope: Complex64,
}

/// A complex-valued function with its derivative (slope).
pub trait ComplexFunctionWithSlope {
    fn eval(&self, z: Complex64) -> ComplexValueAndSlope;
}

/// Renders a Newton's method fractal based on the provided parameters.
pub fn render_newtons_method(
    params: &NewtonsMethodFractalParams,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);
    Ok(())
}
/// Perform one modified Newton–Raphson step:
/// y = z - alpha * f(z) / f'(z)
#[inline]
pub fn modified_newton_raphson_step<F>(z: Complex64, alpha: f64, function: &F) -> Complex64
where
    F: ComplexFunctionWithSlope,
{
    let [value, slope] = {
        let vs = function.eval(z);
        [vs.value, vs.slope]
    };
    let q = value / slope;
    z - q.scale(alpha)
}

/// Real (left-regular) representation of a complex scalar as a 2×2 real matrix.
///
/// Maps s = a + i b to the real-linear map x ↦ s·x on C ≅ R^2:
///     [ a  -b ]
///     [ b   a ]
#[inline]
#[cfg(test)]
fn left_multiply_matrix(s: Complex64) -> Matrix2<f64> {
    Matrix2::new(s.re, -s.im, s.im, s.re)
}

#[cfg(test)]
pub fn assert_consistent_value_and_slope<F: ComplexFunctionWithSlope>(
    function: &F,
    z0: Complex64,
    abs_tol: f64,
    rel_tol: f64,
) {
    // Scaled step size for the finite difference operation
    let scale = (z0.norm() + 1.0).sqrt();
    let h = 1e-7 / scale;

    // central finite differences in x and y
    let dfdx = {
        let f_xp = function.eval(z0 + Complex64::new(h, 0.0)).value;
        let f_xm = function.eval(z0 - Complex64::new(h, 0.0)).value;
        (f_xp - f_xm) * (0.5 / h)
    };
    let dfdy = {
        let f_yp = function.eval(z0 + Complex64::new(0.0, h)).value;
        let f_ym = function.eval(z0 - Complex64::new(0.0, h)).value;
        (f_yp - f_ym) * (0.5 / h)
    };

    // J_num = [[∂u/∂x, ∂u/∂y],
    //          [∂v/∂x, ∂v/∂y]]
    let finite_difference_slope = Matrix2::new(dfdx.re, dfdy.re, dfdx.im, dfdy.im);

    // J_ana = φ(f'(z0))
    let analytic_slope = left_multiply_matrix(function.eval(z0).slope);

    // nalgebra's `.norm()` on matrices is the Frobenius norm (Euclidean of all entries)
    let error_norm = (finite_difference_slope - analytic_slope).norm();
    let reference_scale = analytic_slope.norm().max(1.0);

    assert!(
        error_norm <= abs_tol + rel_tol * reference_scale,
        "Derivative check failed at z0={z0:?}\n\
         numerical J = {finite_difference_slope}\n\
         analytic  J = {analytic_slope}\n\
         err_frob   = {error_norm:e},  bound = {}",
        abs_tol + rel_tol * reference_scale
    );
}

/// Example function: f(z)=z^2 - c, f'(z)=2z
#[cfg(test)]
pub struct QuadraticTestFunction {
    c: Complex64,
}

#[cfg(test)]
impl ComplexFunctionWithSlope for QuadraticTestFunction {
    #[inline]
    fn eval(&self, z: Complex64) -> super::newtons_method::ComplexValueAndSlope {
        super::newtons_method::ComplexValueAndSlope {
            value: z * z - self.c,
            slope: 2.0 * z,
        }
    }
}

#[test]
fn derivative_matches_jacobian_quadratic() {
    let f = QuadraticTestFunction {
        c: Complex64::new(1.0, -0.5),
    };

    // TODO: use image utils to search over a rectangular grid.
    for &z0 in &[
        Complex64::new(0.2, 0.8),
        Complex64::new(-1.3, 0.4),
        Complex64::new(2.0, -1.0),
    ] {
        assert_consistent_value_and_slope(&f, z0, /*abs_tol=*/ 1e-9, /*rel_tol=*/ 1e-7);
    }
}
