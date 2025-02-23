use crate::core::{
    file_io::{serialize_to_json_or_panic, FilePrefix},
    image_utils::{
        generate_scalar_image, write_image_to_file_or_panic, ImageSpecification, RenderOptions,
    },
    ode_solvers::rk4_simulate,
    stopwatch::Stopwatch,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TimePhaseSpecification {
    Snapshot(f64),
    Series { low: f64, upp: f64, count: u32 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DrivenDampedPendulumParams {
    pub image_specification: ImageSpecification,
    pub time_phase: TimePhaseSpecification, // See above.
    // simulation parameters
    pub n_max_period: u32, // maximum number of periods to simulate before aborting
    pub n_steps_per_period: u32,
    // Convergence criteria
    pub periodic_state_error_tolerance: f64,
    pub render_options: RenderOptions,
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
    x_begin: nalgebra::Vector2<f64>,
    time_phase_fraction: f64, // [0, 1] driving function phase offset
    n_max_period: u32,
    n_steps_per_period: u32,
    periodic_state_error_tolerance: f64,
) -> Option<i32> {
    const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
    let t_begin = time_phase_fraction * TWO_PI;
    let t_final = (time_phase_fraction + 1.0) * TWO_PI;
    let mut x = x_begin;
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

pub fn render_driven_damped_pendulum_attractor(
    params: &DrivenDampedPendulumParams,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch = Stopwatch::new("DDP Stopwatch".to_owned());

    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);

    // decide whether to split out to create multiple images, or just continue with a snapshot:
    let time_phase_fraction = match params.time_phase {
        TimePhaseSpecification::Snapshot(time) => time,
        TimePhaseSpecification::Series { low, upp, count } => {
            more_asserts::assert_gt!(count, 0);
            let scale = (upp - low) / (count as f64);
            let inner_directory_path = file_prefix.full_path_with_suffix("");
            std::fs::create_dir_all(&inner_directory_path).unwrap();
            stopwatch.record_split("setup".to_owned());
            for idx in 0..count {
                let time = low + (idx as f64) * scale;
                let mut inner_params = params.clone();
                inner_params.time_phase = TimePhaseSpecification::Snapshot(time);
                let inner_file_prefix = FilePrefix {
                    directory_path: inner_directory_path.clone(),
                    file_base: format!("{}_{}", file_prefix.file_base, idx),
                };
                render_driven_damped_pendulum_attractor(&inner_params, inner_file_prefix)?;
            }
            stopwatch.record_split("simulation".to_owned());
            stopwatch.display(&mut file_prefix.create_file_with_suffix("_diagnostics.txt"))?;

            return Ok(());
        }
    };

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        params.image_specification.resolution[0],
        params.image_specification.resolution[1],
    );

    stopwatch.record_split("setup".to_owned());
    let pixel_renderer = {
        move |point: &nalgebra::Vector2<f64>| {
            let result = compute_basin_of_attraction(
                *point,
                time_phase_fraction,
                params.n_max_period,
                params.n_steps_per_period,
                params.periodic_state_error_tolerance,
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
    };

    let raw_data = generate_scalar_image(
        &params.image_specification,
        &params.render_options,
        pixel_renderer,
        image::Rgb([0, 0, 0]),
    );

    stopwatch.record_split("simulation".to_owned());

    // Iterate over the coordinates and pixels of the image
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = raw_data[x as usize][y as usize];
    }

    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });
    stopwatch.record_split("write_png".to_owned());

    stopwatch.display(&mut file_prefix.create_file_with_suffix("_diagnostics.txt"))?;

    Ok(())
}
