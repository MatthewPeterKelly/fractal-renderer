//! Collection of simple dynamical systems

#[cfg(test)]
use nalgebra::Vector2;

pub struct SimpleLinearControl {
    #[cfg(test)]
    pub xi: f64, // damping ratio
    #[cfg(test)]
    pub omega: f64, // natural frequency
}

impl SimpleLinearControl {
    /// Constant that is used to map between the rise time and omega (natural frequency)
    /// for a critically damped (xi == 1.0) system:
    ///
    /// CRITICALLY_DAMPED_RISE_TIME_SCALE_FACTOR = rise_time * omega;
    pub const CRITICALLY_DAMPED_RISE_TIME_SCALE_FACTOR: f64 = 3.357908561477796;

    /// Computes x(t) for critically damped, overdamped, and underdamped cases
    #[cfg(test)]
    pub fn evaluate_solution(&self, t: f64) -> f64 {
        if self.xi == 1.0 {
            self.critically_damped(t)
        } else if self.xi > 1.0 {
            self.overdamped(t)
        } else {
            self.underdamped(t)
        }
    }

    /// xi == 1.0
    ///
    /// Special case:  what is the rise time of a critically damped system?
    ///
    ///  Define "rise time" as the time to go from 0.1 to 0.9, for a step change
    ///  from an initial condition of zero, to a reference of  1.0.
    ///
    ///  Let omega == 1.0, then we find...
    ///  f(0.5318116083896343) = 0.1
    ///  f(3.88972016986743) = 0.9
    ///  Rise Time: 3.357908561477796
    ///
    #[cfg(test)]
    fn critically_damped(&self, t: f64) -> f64 {
        let w = self.omega;
        (-1.0 - w * t) * (-w * t).exp() + 1.0
    }

    /// xi > 1.0
    #[cfg(test)]
    fn overdamped(&self, t: f64) -> f64 {
        let xi = self.xi;
        let w = self.omega;
        let alpha1 = -xi + (xi * xi - 1.0).sqrt();
        let alpha2 = -xi - (xi * xi - 1.0).sqrt();
        let a = -alpha2 / (alpha2 - alpha1);
        let b = alpha1 / (alpha2 - alpha1);
        a * (alpha1 * w * t).exp() + b * (alpha2 * w * t).exp() + 1.0
    }

    /// xi < 1.0
    #[cfg(test)]
    fn underdamped(&self, t: f64) -> f64 {
        let xi = self.xi;
        let w = self.omega;
        let omega_d = w * (1.0 - xi * xi).sqrt();
        let damping_factor = (-xi * w * t).exp();
        let cosine = (omega_d * t).cos();
        let sine = (omega_d * t).sin();
        damping_factor * (-cosine - (xi / (1.0 - xi * xi).sqrt()) * sine) + 1.0
    }

    /// Implements the model as a discrete linear controller:
    ///
    /// acc = Kp * (x_ref - x) + Kd * (v_ref - v)
    #[cfg(test)]
    pub fn system_dynamics(
        &self,
        reference: &Vector2<f64>,
    ) -> impl Fn(f64, Vector2<f64>) -> Vector2<f64> {
        let k_p = self.omega * self.omega;
        let k_d = 2.0 * self.xi * self.omega;
        let x_ref = reference[0];
        let v_ref = reference[1];

        move |_, state: Vector2<f64>| {
            let x = state[0];
            let v = state[1];

            let v_dot = k_p * (x_ref - x) + k_d * (v_ref - v);
            let x_dot = v;
            Vector2::new(x_dot, v_dot)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_closed_loop_controller_critically_damped_rise_time() {
        let dyn_sys = SimpleLinearControl {
            omega: 1.0,
            xi: 1.0,
        };

        // Regression test on the known rise time properties.
        // Computed offline by a nonlinear root solve.
        let t0 = 0.5318116083896343;
        let t1 = 3.88972016986743;
        let x0 = dyn_sys.evaluate_solution(t0);
        let x1 = dyn_sys.evaluate_solution(t1);

        assert_relative_eq!(x0, 0.1, epsilon = 1e-6);
        assert_relative_eq!(x1, 0.9, epsilon = 1e-6);
        assert_relative_eq!(
            t1 - t0,
            SimpleLinearControl::CRITICALLY_DAMPED_RISE_TIME_SCALE_FACTOR / dyn_sys.omega,
            epsilon = 1e-6
        )
    }
}
