//! Explicit ODE solvers

extern crate nalgebra as na;

// TODO:  move to DDP class
// https://www.dropbox.com/home/mpk/Documents/Random_Projects/Driven_Damped_Pendulum/Version%202?preview=Driven_Damped_Pendulum.m
pub fn driven_damped_pendulum_dynamics(t: f64, x: na::Vector2<f64>) -> na::Vector2<f64> {
    let q = x[0]; // angle
    let v = x[1]; // rate
    let v_dot = t.cos() - 0.1 * v - q.sin();
    na::Vector2::new(v, v_dot)
}

// TODO:  move to DDP class
// Converged if within 0.1 distance of the origin.
// https://www.dropbox.com/home/mpk/Documents/Random_Projects/Driven_Damped_Pendulum/Version%202?preview=Check_Attractor_Status.m
// https://www.dropbox.com/home/mpk/Documents/Random_Projects/Driven_Damped_Pendulum/Version%202?preview=Remove_Attracted_Points.m
// It must be called at the correct phase, ie. cos(t)=1.
pub fn driven_damped_pendulum_attractor(x: na::Vector2<f64>) -> Option<i32> {
    // TODO: how does this correlate to time?
    let basin = na::Vector2::new(-2.0463, 0.3927);
    let delta = x - basin;
    let q = delta[0] % (2.0 * std::f64::consts::PI);
    let v = delta[1];
    let dist = q * q + v * v; // distance of point from attractor
    let r2 = 0.1 * 0.1; // radius-squared around basin
    if dist > r2 {
        // println!("delta: {}, q: {}, v: {}, dist: {}", delta, q, v, dist);
        return None; // outside the basin of attraction
    } else {
        let scale = 0.5 / std::f64::consts::PI;
        let basin_index_flt = delta[0] * scale;
        let basin_index = basin_index_flt as i32;
        // println!("basin_index_flt: {}, basin_index: {}", basin_index_flt, basin_index);
        return Some(basin_index);
    }
}

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

pub fn compute_basin_of_attraction(x_begin: na::Vector2<f64>) -> Option<i32> {
    let n_max_period = 100;
    let n_steps_per_period = 100;
    let t_period = 2.0 * std::f64::consts::PI;
    let mut x = x_begin;
    for _ in 0..n_max_period {
        x = midpoint_simulate(0.0, t_period, n_steps_per_period, x);
        let x_idx = driven_damped_pendulum_attractor(x);
        if let Some(i) = x_idx {
            return Some(i);
        }
    }
    return None;
}

#[cfg(test)]
mod tests {

    #[test]
    fn hello_driven_damped_pendulum_dynamics() {
        extern crate nalgebra as na;
        let x = na::Vector2::new(0.0, 0.0);
        let y = na::Vector2::new(0.0, 1.0);
        assert_relative_eq!(
            crate::ode_solvers::driven_damped_pendulum_dynamics(0.0, x),
            y
        );
    }

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

    #[test]
    fn basin_attraction_test() {
        use crate::ode_solvers::driven_damped_pendulum_attractor;
        use nalgebra::Vector2;

        let basin_center = Vector2::new(-2.0463, 0.3927);
        let basin_offset = Vector2::new(2.0 * std::f64::consts::PI, 0.0);
        let very_fast = Vector2::new(0.0, 9.0);

        let basin_index = driven_damped_pendulum_attractor(basin_center);
        assert_eq!(basin_index, Some(0));
        let basin_index = driven_damped_pendulum_attractor(basin_center + very_fast);
        assert_eq!(basin_index, None);
        let basin_index = driven_damped_pendulum_attractor(basin_center + basin_offset);
        assert_eq!(basin_index, Some(1));
        let basin_index = driven_damped_pendulum_attractor(basin_center - 2.0 * basin_offset);
        assert_eq!(basin_index, Some(-2));
    }

    #[test]
    fn simulate_one_cycle() {
        use crate::ode_solvers::driven_damped_pendulum_attractor;
        use crate::ode_solvers::midpoint_simulate;
        use nalgebra::Vector2;
        {
            // start in the basin
            let basin_center = Vector2::new(-2.0463, 0.3927);
            let x_next = midpoint_simulate(0.0, 2.0 * std::f64::consts::PI, 100, basin_center);
            let basin_index = driven_damped_pendulum_attractor(x_next);
            assert_eq!(basin_index, Some(0));
        }
        {
            // start far from the basin and slowly converge
            let mut x = Vector2::new(-5.0, 6.0);
            let t_period = 2.0 * std::f64::consts::PI;
            for _ in 0..18 {
                x = midpoint_simulate(0.0, t_period, 100, x);
                // println!("i: {}, x: {}", i, x);
                let x_idx = driven_damped_pendulum_attractor(x);
                assert_eq!(x_idx, None);
            }
            // regression check: here is where we expect to converge
            x = midpoint_simulate(0.0, t_period, 100, x);
            let x_idx = driven_damped_pendulum_attractor(x);
            assert_eq!(x_idx, Some(4));
        }
    }

    #[test]
    fn test_check_basin() {
        use crate::ode_solvers::compute_basin_of_attraction;
        use nalgebra::Vector2;
        {
            let basin_center = Vector2::new(-2.0463, 0.3927);
            let x_idx = compute_basin_of_attraction(basin_center);
            assert_eq!(x_idx, Some(0));
        }
        {
            let x_idx = compute_basin_of_attraction(Vector2::new(-5.0, 6.0));
            assert_eq!(x_idx, Some(4));
        }
        {
            let x_idx = compute_basin_of_attraction(Vector2::new(-2.0, 9.0));
            assert_eq!(x_idx, Some(11));
        }
    }
}
