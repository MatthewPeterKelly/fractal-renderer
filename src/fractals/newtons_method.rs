// Common library funcions for fractals that are backed by Newton's method,
// such as the roots of unity fractal.

use num_complex::Complex;
use num_traits::Float;

/// Perform a single iteration of the modified Newton-Rhapson method:
/// y = z - alpha * value(z) / slope(z)
/// where:
///   `z` is the current point
///   `alpha` is a scaling factor (the "modified" part of the method)
///   `value` is the function that we're trying to find the root of
///   `slope` is the derivative of the function with respect to z
#[inline]
pub fn modified_newton_rhapson_step<T, F, G>(
    z: Complex<T>,
    alpha: T,
    value: F,
    slope: G,
) -> Complex<T>
where
    T: Float,
    F: Fn(Complex<T>) -> Complex<T>,
    G: Fn(Complex<T>) -> Complex<T>,
{
    let q = value(z) / slope(z);
    z - q.scale(alpha)
}
