//! Explicit ODE solvers

extern crate nalgebra as na;

pub fn rk4_method_step<F>(dt: f64, t: f64, x: na::Vector2<f64>, dynamics: &F) -> na::Vector2<f64>
where
    F: Fn(f64, na::Vector2<f64>) -> na::Vector2<f64>,
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
    x0: na::Vector2<f64>, dynamics:& F
) -> na::Vector2<f64>
where
    F: Fn(f64, na::Vector2<f64>) -> na::Vector2<f64>,{
    let dt = (t_final - t_begin) / (n_steps as f64);
    let mut x = x0;
    for i_step in 0..n_steps {
        let alpha = (i_step as f64) / (n_steps as f64);
        let t = t_begin + alpha * (t_final - t_begin);
        x = rk4_method_step(dt, t, x, dynamics);
    }
    x
}
