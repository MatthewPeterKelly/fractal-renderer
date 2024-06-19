use crate::{file_io, ode_solvers::rk4_simulate};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::render;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TimePhaseSpecification {
    Snapshot(f64),
    Series { low: f64, upp: f64, count: u32 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DrivenDampedPendulumParams {
    pub image_specification: render::ImageSpecification,
    pub time_phase: TimePhaseSpecification, // See above.
    // simulation parameters
    pub n_max_period: u32, // maximum number of periods to simulate before aborting
    pub n_steps_per_period: u32,
    // Convergence criteria
    pub periodic_state_error_tolerance: f64,
    // Anti-aliasing when n > 1. Expensive, but huge improvement to image quality
    // 1 == no antialiasing
    // 3 = some antialiasing (at 9x CPU time)
    // 7 = high antialiasing (at cost of 49x CPU time)
    pub subpixel_antialiasing: u32,
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
        x = rk4_simulate(t_begin, t_final, n_steps_per_period, x_prev);
        let x_idx = driven_damped_pendulum_attractor(x, x_prev, periodic_state_error_tolerance);
        if let Some(i) = x_idx {
            return Some(i);
        }
    }
    None
}

#[derive(Default)]
pub struct MeasuredElapsedTime {
    pub setup: Duration,
    pub simulation: Duration,
    pub write_png: Duration,
}

impl MeasuredElapsedTime {
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "MeasuredElapsedTime:")?;
        writeln!(writer, " -- Setup:      {:?}", self.setup)?;
        writeln!(writer, " -- Simulation: {:?}", self.simulation)?;
        writeln!(writer, " -- Write PNG:  {:?}", self.write_png)?;
        writeln!(writer)?;
        Ok(())
    }
}

pub fn render_driven_damped_pendulum_attractor(
    params: &DrivenDampedPendulumParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    // write out the parameters to a file:
    let params_path = file_prefix.with_suffix(".json");
    let params_str = serde_json::to_string(params)?;
    std::fs::write(params_path, params_str).expect("Unable to write params file.");

    // decide whether to split out to create multiple images, or just continue with a snapshot:
    let time_phase_fraction = match params.time_phase {
        TimePhaseSpecification::Snapshot(time) => time,
        TimePhaseSpecification::Series { low, upp, count } => {
            more_asserts::assert_gt!(count, 0);
            let scale = (upp - low) / (count as f64);
            let inner_directory_path = file_prefix.directory_path.join("series");
            std::fs::create_dir_all(&inner_directory_path).unwrap();

            timer.setup = render::elapsed_and_reset(&mut stopwatch);
            for idx in 0..count {
                let time = low + (idx as f64) * scale;
                let mut inner_params = params.clone();
                inner_params.time_phase = TimePhaseSpecification::Snapshot(time);
                let inner_file_prefix = file_io::FilePrefix {
                    directory_path: inner_directory_path.clone(),
                    file_base: format!("{}_{}", file_prefix.file_base, idx),
                };
                render_driven_damped_pendulum_attractor(&inner_params, &inner_file_prefix)?;
            }
            timer.simulation = render::elapsed_and_reset(&mut stopwatch);
            timer.display(&mut file_prefix.create_file_with_suffix("_diagnostics.txt"))?;
            return Ok(());
        }
    };

    let render_path = file_prefix.with_suffix(".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        params.image_specification.resolution[0],
        params.image_specification.resolution[1],
    );

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    let subpixel_samples = params
        .image_specification
        .subpixel_offset_vector(params.subpixel_antialiasing);
    let subpixel_scale = 1.0 / (subpixel_samples.len() as f64);

    let pixel_renderer = {
        let subpixel_samples = &subpixel_samples; // Capture by reference
        move |point: &nalgebra::Vector2<f64>| {
            let mut sum = 0.0;

            for sample in subpixel_samples {
                let result = compute_basin_of_attraction(
                    nalgebra::Vector2::<f64>::new(point[0] + sample[0], point[1] + sample[1]),
                    time_phase_fraction,
                    params.n_max_period,
                    params.n_steps_per_period,
                    params.periodic_state_error_tolerance,
                );
                if Option::<i32>::Some(0) == result {
                    sum += subpixel_scale;
                }
            }
            sum
        }
    };

    let raw_data = render::generate_scalar_image(&params.image_specification, pixel_renderer);

    timer.simulation = render::elapsed_and_reset(&mut stopwatch);

    // Iterate over the coordinates and pixels of the image
    let color_map = greyscale_color_map();
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = color_map(raw_data[x as usize][y as usize]);
    }

    // Save the image to a file, deducing the type from the file name
    imgbuf.save(&render_path).unwrap();
    timer.write_png = render::elapsed_and_reset(&mut stopwatch);

    println!("Wrote image file to: {}", render_path.display());

    timer.display(&mut file_prefix.create_file_with_suffix("_diagnostics.txt"))?;

    Ok(())
}

fn greyscale_color_map() -> impl Fn(f64) -> image::Rgb<u8> {
    move |input: f64| {
        let value = (input * 255.0) as u8;
        image::Rgb([value, value, value])
    }
}
