//! Collection of simple dynamical systems

/// The `SimpleLinearControl` class is used as a canonical test system
/// for the ODE solvers: it is an interesting system with non-trivial
/// dynamics and a known analytic solution.
#[cfg(test)]
use nalgebra::Vector2;
#[cfg(test)]
pub struct SimpleLinearControl {
    #[cfg(test)]
    pub xi: f64, // damping ratio
    #[cfg(test)]
    pub omega: f64, // natural frequency
}

#[cfg(test)]
impl SimpleLinearControl {
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
