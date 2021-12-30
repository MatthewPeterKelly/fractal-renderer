//! Explicit ODE solvers

extern crate nalgebra as na;
use crate::ddp_utils::driven_damped_pendulum_dynamics; // HACK

// TODO:  pass dynamics function as argument
// TODO:  upgrade to RK4:
// https://www.dropbox.com/home/mpk/Documents/Random_Projects/Driven_Damped_Pendulum/Version%202?preview=Runge_Kutta_MPK.m
pub fn euler_step(dt: f64, t: f64, x: na::Vector2<f64>) -> na::Vector2<f64> {
    let x_dot = driven_damped_pendulum_dynamics(t, x);
    x + dt * x_dot
}

pub fn midpoint_method_step(dt: f64, t: f64, x: na::Vector2<f64>) -> na::Vector2<f64> {
    let t_mid = t + 0.5 * dt;
    let x_mid = x + 0.5 * dt * driven_damped_pendulum_dynamics(t, x);
    x + dt * driven_damped_pendulum_dynamics(t_mid, x_mid)
}

pub fn midpoint_simulate(
    t_begin: f64,
    t_final: f64,
    n_steps: i32,
    x0: na::Vector2<f64>,
) -> na::Vector2<f64> {
    let dt = (t_final - t_begin) / (n_steps as f64);
    let mut x = x0;
    for i_step in 0..n_steps {
        let alpha = (i_step as f64) / (n_steps as f64);
        let t = t_begin + alpha * (t_final - t_begin);
        x = midpoint_method_step(dt, t, x);
    }
    x
}

#[cfg(test)]
mod tests {

    #[test]
    fn hello_euler_step() {
        extern crate nalgebra as na;
        let x = na::Vector2::new(0.0, 0.0);
        let t = 0.0;
        let dt = 0.001;
        let x_next = crate::ode_solvers::euler_step(dt, t, x);
        let x_soln = na::Vector2::new(0.0, 0.001);
        assert_relative_eq!(x_next, x_soln);
    }
}
