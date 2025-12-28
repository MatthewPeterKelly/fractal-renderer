use crate::core::{
    image_utils::{
        scale_down_parameter_for_speed, scale_up_parameter_for_speed, ImageSpecification,
        RenderOptions, Renderable, SpeedOptimizer,
    },
    interpolation::{ClampedLinearInterpolator, ClampedLogInterpolator},
    ode_solvers::rk4_simulate,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DrivenDampedPendulumParams {
    pub image_specification: ImageSpecification,
    // dynamical system parameters:
    pub time_phase: f64,
    // simulation parameters
    pub n_max_period: u32, // maximum number of periods to simulate before aborting
    pub n_steps_per_period: u32,
    // Convergence criteria
    pub periodic_state_error_tolerance: f64,
    pub render_options: RenderOptions,
}

impl Renderable for DrivenDampedPendulumParams {
    type Params = DrivenDampedPendulumParams;

    fn render_point(&self, point: &[f64; 2]) -> image::Rgb<u8> {
        let result = compute_basin_of_attraction(
            point,
            self.time_phase,
            self.n_max_period,
            self.n_steps_per_period,
            self.periodic_state_error_tolerance,
        );
        // We color the pixel white if it is in the zeroth basin of attraction.
        // Otherwise, color it black. Alternative coloring schemes could be:
        // - color each basin a different color.
        // - grayscale based on angular distance traveled to reach stable orbit
        if result == Some(0) {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([0, 0, 0])
        }
    }

    fn image_specification(&self) -> &ImageSpecification {
        &self.image_specification
    }

    fn render_options(&self) -> &RenderOptions {
        &self.render_options
    }

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        self.image_specification = image_specification;
    }

    fn write_diagnostics<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        std::io::Result::Ok(())
    }

    fn params(&self) -> &Self::Params {
        self
    }

    fn render_to_buffer(&self, buffer: &mut Vec<Vec<image::Rgb<u8>>>) {
        crate::core::image_utils::generate_scalar_image_in_place(
            self.image_specification(),
            self.render_options(),
            |point: &[f64; 2]| self.render_point(point),
            buffer,
        );
    }
}

pub struct ParamsReferenceCache {
    pub n_max_period: u32,
    pub n_steps_per_period: u32,
    pub periodic_state_error_tolerance: f64,
    pub render_options: RenderOptions,
}

impl SpeedOptimizer for DrivenDampedPendulumParams {
    type ReferenceCache = ParamsReferenceCache;

    fn reference_cache(&self) -> Self::ReferenceCache {
        ParamsReferenceCache {
            n_max_period: self.n_max_period,
            n_steps_per_period: self.n_steps_per_period,
            periodic_state_error_tolerance: self.periodic_state_error_tolerance,
            render_options: self.render_options,
        }
    }

    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache) {
        self.n_max_period = scale_down_parameter_for_speed(
            16.0,
            cache.n_max_period as f64,
            level,
            ClampedLinearInterpolator,
        ) as u32;

        self.n_steps_per_period = scale_down_parameter_for_speed(
            128.0,
            cache.n_steps_per_period as f64,
            level,
            ClampedLogInterpolator,
        ) as u32;

        self.periodic_state_error_tolerance = scale_up_parameter_for_speed(
            1e-2,
            cache.periodic_state_error_tolerance,
            level,
            ClampedLogInterpolator,
        );

        self.render_options
            .set_speed_optimization_level(level, &cache.render_options);
    }
}

/**
 * Based on implementation from:
 * https://www.dropbox.com/home/mpk/Documents/Random_Projects/Driven_Damped_Pendulum/Version%202?preview=Driven_Damped_Pendulum.m
 *
 * Computes the system dynamics of the "canonical" driven-damped pendulum.
 *
 * Note: hard-codes all parameters, eventually it might be nice to generalize it.
 */
pub fn driven_damped_pendulum_dynamics(
    t: f64,
    x: nalgebra::Vector2<f64>,
) -> nalgebra::Vector2<f64> {
    let q = x[0]; // angle
    let v = x[1]; // rate
    let v_dot = t.cos() - 0.1 * v - q.sin();
    nalgebra::Vector2::new(v, v_dot)
}

// TODO:  move to DDP class
// This function should be called in-phase with the driving function.
// The exact phase is not important, only that it is consistent.
pub fn driven_damped_pendulum_attractor(
    x: nalgebra::Vector2<f64>,
    x_prev: nalgebra::Vector2<f64>,
    tol: f64,
) -> Option<i32> {
    let delta = x - x_prev;
    let err_n2 = delta.dot(&delta);
    if err_n2 > tol {
        None // outside the basin of attraction
    } else {
        Some(compute_basin_index(x[0]))
    }
}

pub fn compute_basin_index(angle: f64) -> i32 {
    const SCALE_TO_UNITY: f64 = 0.5 / std::f64::consts::PI;
    (angle * SCALE_TO_UNITY).round() as i32
}

// TODO:  this should return a custom data structure that includes a variety of
// information, all of which gets saved to the data set.
// - iteration count
// - basin at termination
// - termination type (converged, max iter)
pub fn compute_basin_of_attraction(
    x_begin: &[f64; 2],
    time_phase_fraction: f64, // [0, 1] driving function phase offset
    n_max_period: u32,
    n_steps_per_period: u32,
    periodic_state_error_tolerance: f64,
) -> Option<i32> {
    const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
    let t_begin = time_phase_fraction * TWO_PI;
    let t_final = (time_phase_fraction + 1.0) * TWO_PI;
    let mut x = nalgebra::Vector2::new(x_begin[0], x_begin[1]);
    for _ in 0..n_max_period {
        let x_prev = x;
        x = rk4_simulate(
            t_begin,
            t_final,
            n_steps_per_period,
            x_prev,
            &driven_damped_pendulum_dynamics,
        );
        let x_idx = driven_damped_pendulum_attractor(x, x_prev, periodic_state_error_tolerance);
        if let Some(i) = x_idx {
            return Some(i);
        }
    }
    None
}
