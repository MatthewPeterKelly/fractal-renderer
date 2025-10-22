// Common library funcions for fractals that are backed by Newton's method,
// such as the roots of unity fractal.

use num_complex::Complex64;

/// A complex-valued function with its derivative (slope).
pub trait ComplexFunctionWithSlope {
    /// f(z)
    fn value(&self, z: Complex64) -> Complex64;

    /// f'(z)
    fn slope(&self, z: Complex64) -> Complex64;
}

/// Perform one modified Newton–Raphson step:
/// y = z - alpha * f(z) / f'(z)
#[inline]
pub fn modified_newton_raphson_step<F>(z: Complex64, alpha: f64, function: &F) -> Complex64
where
    F: ComplexFunctionWithSlope,
{
    let q = function.value(z) / function.slope(z);
    z - q.scale(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex64;

    /// TODO:  docs
    fn assert_consistent_value_and_slope<F: ComplexFunctionWithSlope>(
        function: &F,
        z0: Complex64,
        abs_tol: f64,
        rel_tol: f64,
    ) {
        // TODO:  look into this....
        // Small step for central differences. Scale with |z0| to keep it relative.
        let scale = (z0.norm() + 1.0).sqrt(); // mild scaling for stability
        let h = 1e-7 / scale;

        // Central difference along real axis: df/dx ≈ (f(z0+h) - f(z0-h)) / (2h)
        let f_x_plus = function.value(z0 + Complex64::new(h, 0.0));
        let f_x_minus = function.value(z0 - Complex64::new(h, 0.0));
        let dfdx = (f_x_plus - f_x_minus) * (0.5 / h);

        // Central difference along imaginary axis: df/dy where we perturb y via i*h
        let f_y_plus = function.value(z0 + Complex64::new(0.0, h));
        let f_y_minus = function.value(z0 - Complex64::new(0.0, h));
        let dfdy = (f_y_plus - f_y_minus) * (0.5 / h);

        // Build numerical Jacobian J_num = [[∂u/∂x, ∂u/∂y], [∂v/∂x, ∂v/∂y]]
        // with f = u + i v and dfdx = ∂u/∂x + i ∂v/∂x, dfdy = ∂u/∂y + i ∂v/∂y
        let j_num = [[dfdx.re, dfdy.re], [dfdx.im, dfdy.im]];

        // Analytic derivative from the slope method
        let s = function.slope(z0);
        let j_ana = [[s.re, -s.im], [s.im, s.re]];

        // TODO:  better comparison?
        // Use nalagebra?

        // Frobenius norms
        let frob = |m: [[f64; 2]; 2]| -> f64 {
            (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[1][0] * m[1][0] + m[1][1] * m[1][1]).sqrt()
        };
        let sub = |a: [[f64; 2]; 2], b: [[f64; 2]; 2]| -> [[f64; 2]; 2] {
            [
                [a[0][0] - b[0][0], a[0][1] - b[0][1]],
                [a[1][0] - b[1][0], a[1][1] - b[1][1]],
            ]
        };

        let diff = sub(j_num, j_ana);
        let err = frob(diff);
        let refn = frob(j_ana).max(1.0);

        assert!(
            err <= abs_tol + rel_tol * refn,
            "Derivative check failed at z0={z0:?}\n\
             numerical J = {j_num:?}\n\
             analytic  J = {j_ana:?}\n\
             err_frob   = {err:e},  bound = {}",
            abs_tol + rel_tol * refn
        );
    }

    /// Example function: f(z)=z^2 - c, f'(z)=2z
    struct Quadratic {
        c: Complex64,
    }
    impl ComplexFunctionWithSlope for Quadratic {
        #[inline]
        fn value(&self, z: Complex64) -> Complex64 {
            z * z - self.c
        }
        #[inline]
        fn slope(&self, z: Complex64) -> Complex64 {
            2.0 * z
        }
    }

    #[test]
    fn derivative_matches_jacobian_quadratic() {
        let c = Complex64::new(1.0, -0.5);
        let f = Quadratic { c };

        // Try a few points (can add more if you like)
        for &z0 in &[
            Complex64::new(0.2, 0.8),
            Complex64::new(-1.3, 0.4),
            Complex64::new(2.0, -1.0),
        ] {
            assert_holomorphic_derivative_ok(
                &f, z0, /*abs_tol=*/ 1e-9, /*rel_tol=*/ 1e-7,
            );
        }
    }

    /// Example using the tuple-of-closures convenience impl (still generic/no `dyn`)
    #[test]
    fn derivative_matches_jacobian_closures() {
        let c = Complex64::new(-0.3, 0.7);
        let funcs = (move |z: Complex64| z * z - c, |z: Complex64| 2.0 * z);
        for &z0 in &[Complex64::new(0.1, -0.2), Complex64::new(0.9, 0.3)] {
            assert_holomorphic_derivative_ok(&funcs, z0, 1e-9, 1e-7);
        }
    }
}
