use crate::ode_solvers::rk4_simulate;
use rayon::prelude::{IntoParallelIterator, ParallelExtend, ParallelIterator};
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
    // Where to render?
    pub image_resolution: nalgebra::Vector2<u32>,
    pub center: nalgebra::Vector2<f64>,
    pub angle_scale: f64,                   // angle_max - angle_min
    pub time_phase: TimePhaseSpecification, // See above.
    // simulation parameters
    pub n_max_period: u32, // maximum number of periods to simulate before aborting
    pub n_steps_per_period: u32,
    // Convergence criteria
    pub periodic_state_error_tolerance: f64,
}

impl Default for DrivenDampedPendulumParams {
    fn default() -> DrivenDampedPendulumParams {
        DrivenDampedPendulumParams {
            image_resolution: nalgebra::Vector2::<u32>::new(400, 300),
            center: nalgebra::Vector2::<f64>::new(0.0, 0.0),
            angle_scale: std::f64::consts::TAU,
            time_phase: TimePhaseSpecification::Snapshot(0.0),
            n_max_period: (100),
            n_steps_per_period: (10),
            periodic_state_error_tolerance: (1e-4),
        }
    }
}

impl DrivenDampedPendulumParams {
    pub fn rate_scale(&self) -> f64 {
        self.angle_scale * (self.image_resolution[1] as f64) / (self.image_resolution[0] as f64)
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
    directory_path: &std::path::Path,
    file_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    // write out the parameters to a file:
    let params_path = directory_path.join(file_prefix.to_owned() + ".json");
    let params_str = serde_json::to_string(params)?;
    std::fs::write(params_path, params_str).expect("Unable to write params file.");

    // decide whether to split out to create multiple images, or just continue with a snapshot:
    let time_phase_fraction = match params.time_phase {
        TimePhaseSpecification::Snapshot(time) => time,
        TimePhaseSpecification::Series { low, upp, count } => {
            more_asserts::assert_gt!(count, 0);
            let scale = (upp - low) / (count as f64);
            let inner_directory_path = directory_path.join("series");
            std::fs::create_dir_all(&inner_directory_path).unwrap();

            timer.setup = render::elapsed_and_reset(&mut stopwatch);
            for idx in 0..count {
                let time = low + (idx as f64) * scale;
                let mut inner_params = params.clone();
                inner_params.time_phase = TimePhaseSpecification::Snapshot(time);
                render_driven_damped_pendulum_attractor(
                    &inner_params,
                    &inner_directory_path,
                    &format!("{}_{}", file_prefix, idx),
                )?;
            }
            timer.simulation = render::elapsed_and_reset(&mut stopwatch);
            let file = std::fs::File::create(
                directory_path.join(file_prefix.to_owned() + "_diagnostics.txt"),
            )
            .expect("failed to create diagnostics file");
            let mut diagnostics_file = std::io::BufWriter::new(file);
            timer.display(&mut diagnostics_file)?;
            return Ok(());
        }
    };

    let render_path = directory_path.join(file_prefix.to_owned() + ".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf =
        image::ImageBuffer::new(params.image_resolution[0], params.image_resolution[1]);

    // Mapping from image space to complex space
    let pixel_map_real = render::LinearPixelMap::new_from_center_and_width(
        params.image_resolution[0],
        params.center[0],
        params.angle_scale,
    );
    let pixel_map_imag = render::LinearPixelMap::new_from_center_and_width(
        params.image_resolution[1],
        params.center[1],
        -params.rate_scale(), // Image coordinates are upside down.
    );

    // Note:  everything above this could be shared, as well as some other stuff

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    // Generate the raw data for the fractal, using Rayon to parallelize the calculation.
    let mut raw_data: Vec<Vec<f64>> = Vec::with_capacity(params.image_resolution[0] as usize);
    raw_data.par_extend((0..params.image_resolution[0]).into_par_iter().map(|x| {
        let angle = pixel_map_real.map(x);
        (0..params.image_resolution[1])
            .map(|y| {
                let rate = pixel_map_imag.map(y);
                let result = compute_basin_of_attraction(
                    nalgebra::Vector2::<f64>::new(angle, rate),
                    time_phase_fraction,
                    params.n_max_period,
                    params.n_steps_per_period,
                    params.periodic_state_error_tolerance,
                );
                if Option::<i32>::Some(0) == result {
                    1.0
                } else {
                    0.0
                }
            })
            .collect()
    }));

    timer.simulation = render::elapsed_and_reset(&mut stopwatch);

    // Iterate over the coordinates and pixels of the image
    let color_map = simple_black_and_white_color_map();
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = color_map(raw_data[x as usize][y as usize]);
    }

    // Save the image to a file, deducing the type from the file name
    imgbuf.save(&render_path).unwrap();
    timer.write_png = render::elapsed_and_reset(&mut stopwatch);

    println!("Wrote image file to: {}", render_path.display());

    let file =
        std::fs::File::create(directory_path.join(file_prefix.to_owned() + "_diagnostics.txt"))
            .expect("failed to create diagnostics file");
    let mut diagnostics_file = std::io::BufWriter::new(file);
    timer.display(&mut diagnostics_file)?;

    Ok(())
}

fn simple_black_and_white_color_map() -> impl Fn(f64) -> image::Rgb<u8> {
    move |input: f64| {
        const THRESHOLD: f64 = 0.5;
        if input > THRESHOLD {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([0, 0, 0])
        }
    }
}
