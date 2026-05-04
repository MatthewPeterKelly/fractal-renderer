use crate::core::{
    color_map::{ColorMap, ColorMapKeyFrame},
    field_iteration::FieldKernel,
    image_utils::{
        ImageSpecification, RenderOptions, Renderable, SpeedOptimizer,
        scale_down_parameter_for_speed, scale_up_parameter_for_speed,
    },
    interpolation::{ClampedLinearInterpolator, ClampedLogInterpolator},
    ode_solvers::rk4_simulate,
};
use serde::{Deserialize, Serialize};

/// Default color map for DDP: black flat color (out-of-basin) and a
/// degenerate single-keyframe-pair white gradient (zeroth-basin).
/// Matches the previously hard-coded foreground/background, so JSON files
/// without a `color` field continue to render identically.
fn ddp_default_color() -> ColorMap {
    ColorMap {
        flat_color: [0, 0, 0],
        gradients: vec![vec![
            ColorMapKeyFrame {
                query: 0.0,
                rgb_raw: [255, 255, 255],
            },
            ColorMapKeyFrame {
                query: 1.0,
                rgb_raw: [255, 255, 255],
            },
        ]],
    }
}

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
    /// Flat (out-of-basin) color and a single-gradient (in-basin) palette.
    /// The gradient is constant-color in the canonical configuration, so
    /// the histogram / CDF percentile output never affects pixels.
    #[serde(default = "ddp_default_color")]
    pub color: ColorMap,
}

impl FieldKernel for DrivenDampedPendulumParams {
    /// Map "in zeroth basin" to a `Some((1.0, 0))` cell — value `1.0`
    /// trivially fills the single histogram bin, and gradient index 0
    /// routes to DDP's only gradient. Out-of-basin / non-converged → `None`,
    /// which colorizes through `flat_color`.
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
        match compute_basin_of_attraction(
            &point,
            self.time_phase,
            self.n_max_period,
            self.n_steps_per_period,
            self.periodic_state_error_tolerance,
        ) {
            Some(0) => Some((1.0, 0)),
            _ => None,
        }
    }
}

impl Renderable for DrivenDampedPendulumParams {
    type Params = DrivenDampedPendulumParams;

    fn color_map(&self) -> &ColorMap {
        &self.color
    }

    fn color_map_mut(&mut self) -> &mut ColorMap {
        &mut self.color
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
        Ok(())
    }

    fn params(&self) -> &Self::Params {
        self
    }

    /// DDP's gradient is constant-color, so the histogram output never
    /// affects pixels — a single bin is sufficient.
    fn histogram_bin_count(&self) -> usize {
        1
    }

    /// Likewise, the histogram's max value is irrelevant for DDP.
    fn histogram_max_value(&self) -> f32 {
        1.0
    }

    /// LUT resolution for the (constant-color) gradient. Small value to
    /// keep allocation trivial.
    fn lookup_table_count(&self) -> usize {
        4
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A pre-Phase-1 DDP params JSON has no `color` field. The
    /// `#[serde(default)]` shim must fill it with the degenerate
    /// white-on-black gradient — matching the previously hard-coded
    /// values — so existing files render identically.
    #[test]
    fn parses_legacy_json_without_color_field_with_default_white_black() {
        let json = r#"{
            "image_specification": {
                "resolution": [400, 200],
                "center": [0, 0],
                "width": 14
            },
            "time_phase": 0,
            "n_max_period": 50,
            "n_steps_per_period": 12,
            "periodic_state_error_tolerance": 0.05,
            "render_options": {
                "sampling_level": 1
            }
        }"#;
        let parsed: DrivenDampedPendulumParams = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.color.flat_color, [0, 0, 0]);
        assert_eq!(parsed.color.gradients.len(), 1);
        assert_eq!(parsed.color.gradients[0].len(), 2);
        assert_eq!(parsed.color.gradients[0][0].rgb_raw, [255, 255, 255]);
        assert_eq!(parsed.color.gradients[0][1].rgb_raw, [255, 255, 255]);
    }
}
