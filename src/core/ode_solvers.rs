//! Explicit ODE solvers

use nalgebra::Vector2;

pub fn rk4_method_step<F>(dt: f64, t: f64, x: Vector2<f64>, dynamics: &F) -> Vector2<f64>
where
    F: Fn(f64, Vector2<f64>) -> Vector2<f64>,
{
    let t_mid = t + 0.5 * dt;
    let t_next = t + dt;
    let k1 = dt * dynamics(t, x);
    let k2 = dt * dynamics(t_mid, x + 0.5 * k1);
    let k3 = dt * dynamics(t_mid, x + 0.5 * k2);
    let k4 = dt * dynamics(t_next, x + k3);
    const ONE_BY_SIX: f64 = 1.0 / 6.0;
    let x_delta = ONE_BY_SIX * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
    x + x_delta
}

pub fn rk4_simulate<F>(
    t_begin: f64,
    t_final: f64,
    n_steps: u32,
    x0: Vector2<f64>,
    dynamics: &F,
) -> Vector2<f64>
where
    F: Fn(f64, Vector2<f64>) -> Vector2<f64>,
{
    let dt = (t_final - t_begin) / (n_steps as f64);
    let mut x = x0;
    for i_step in 0..n_steps {
        let alpha = (i_step as f64) / (n_steps as f64);
        let t = t_begin + alpha * (t_final - t_begin);
        x = rk4_method_step(dt, t, x, dynamics);
    }
    x
}

#[cfg(test)]
mod tests {
    use crate::core::dynamical_systems::SimpleLinearControl;

    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_closed_loop_controller_analytic_soln() {
        let natural_frequency = 2.0;
        let damping_ratio_test_values = [1.0, 1.2, 0.8]; // Critically damped, overdamped, underdamped
        let t_begin = 0.0;
        let t_final = 3.0;
        let n_steps = 500;
        let dt = (t_final - t_begin) / (n_steps as f64);

        let target_state = Vector2::new(1.0, 0.0);

        for &damping_ratio in &damping_ratio_test_values {
            let control_model = SimpleLinearControl {
                omega: natural_frequency,
                xi: damping_ratio,
            };

            // Define system dynamics and analytical solution
            let dynamics = control_model.system_dynamics(&target_state);
            let analytical_solution = |t: f64| control_model.evaluate_solution(t);

            // Run RK4 simulation
            let mut state = Vector2::new(0.0, 0.0);
            let pos_err_tol = 1e-3;
            for i in 0..=n_steps {
                let t = t_begin + (i as f64) * dt;

                // Compare numerical solution (state[0]) with analytical solution
                assert_relative_eq!(state[0], analytical_solution(t), epsilon = pos_err_tol);

                // Step forward using RK4
                state = rk4_method_step(dt, t, state, &dynamics);
            }
        }
    }
}
