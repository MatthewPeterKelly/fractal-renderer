//! Explicit ODE solvers

extern crate nalgebra as na;
use crate::ode_solvers::rk4_simulate;
use serde::{Deserialize, Serialize};

// TODO:  add constructor for this!!!
#[derive(Serialize, Deserialize, Debug)]
pub struct FractalRawData {
    pub angle_count: u32, // note: duplicates matrix dimensions...
    pub rate_count: u32,  // note: duplicates matrix dimensions...
    pub max_rate: f64,    // for data file only
    pub data: na::DMatrix<i32>,
}

// TODO:  move to DDP class
// https://www.dropbox.com/home/mpk/Documents/Random_Projects/Driven_Damped_Pendulum/Version%202?preview=Driven_Damped_Pendulum.m
pub fn driven_damped_pendulum_dynamics(t: f64, x: na::Vector2<f64>) -> na::Vector2<f64> {
    let q = x[0]; // angle
    let v = x[1]; // rate
    let v_dot = t.cos() - 0.1 * v - q.sin();
    na::Vector2::new(v, v_dot)
}

// TODO:  move to DDP class
// This function should be called in-phase with the driving function.
// The exact phase is not important, only that it is consistent.
pub fn driven_damped_pendulum_attractor(
    x: na::Vector2<f64>,
    x_prev: na::Vector2<f64>,
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
pub fn compute_basin_of_attraction(x_begin: na::Vector2<f64>) -> Option<i32> {
    let n_max_period = 500;
    let n_steps_per_period = 50;
    const T_PERIOD: f64 = 2.0 * std::f64::consts::PI;
    let tol = 0.005;
    let mut x = x_begin;
    for _ in 0..n_max_period {
        let x_prev = x;
        x = rk4_simulate(0.0, T_PERIOD, n_steps_per_period, x_prev);
        let x_idx = driven_damped_pendulum_attractor(x, x_prev, tol);
        if let Some(i) = x_idx {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {

    #[test]
    fn hello_driven_damped_pendulum_dynamics() {
        extern crate nalgebra as na;
        let x = na::Vector2::new(0.0, 0.0);
        let y = na::Vector2::new(0.0, 1.0);
        approx::assert_relative_eq!(crate::ddp_utils::driven_damped_pendulum_dynamics(0.0, x), y);
    }

    #[test]
    fn simulate_one_cycle() {
        use crate::ddp_utils::driven_damped_pendulum_attractor;
        use crate::ode_solvers::midpoint_simulate;
        use nalgebra::Vector2;
        {
            // start in the basin
            let basin_center = Vector2::new(-2.05, 0.39);
            let x_next = midpoint_simulate(0.0, 2.0 * std::f64::consts::PI, 100, basin_center);
            let basin_index = driven_damped_pendulum_attractor(x_next, basin_center, 0.5);
            assert_eq!(basin_index, Some(0));
        }
        {
            // start far from the basin and slowly converge
            let mut x = Vector2::new(-5.0, 6.0);
            let t_period = 2.0 * std::f64::consts::PI;
            for _i in 0..23 {
                let x_prev = x;
                x = midpoint_simulate(0.0, t_period, 100, x);
                let x_idx = driven_damped_pendulum_attractor(x, x_prev, 0.01);
                // println!("i: {}, x: {}, x_idx: {:?}", _i, x, x_idx);
                if let Some(core) = x_idx {
                    assert_eq!(core, 4);
                } else {
                    assert_eq!(x_idx, None);
                }
            }
        }
    }

    #[test]
    fn test_check_basin() {
        use crate::ddp_utils::compute_basin_of_attraction;
        use nalgebra::Vector2;
        {
            let basin_center = Vector2::new(-2.05, 0.39);
            let x_idx = compute_basin_of_attraction(basin_center);
            assert_eq!(x_idx, Some(0));
        }
        {
            let x_idx = compute_basin_of_attraction(Vector2::new(-5.0, 6.0));
            assert_eq!(x_idx, Some(6));
        }
        {
            let x_idx = compute_basin_of_attraction(Vector2::new(-2.0, 9.0));
            assert_eq!(x_idx, Some(11));
        }
    }

    // TODO:  use binary encoding:  https://crates.io/crates/bincode

    #[test]
    fn basic_serialization_demo() {
        use crate::ddp_utils::FractalRawData;
        use nalgebra::DMatrix;
        let mut fractal_raw_data = FractalRawData {
            angle_count: 10,
            rate_count: 20,
            max_rate: 2.0,
            data: DMatrix::from_element(10, 20, 0),
        };

        fractal_raw_data.data[(0, 5)] = -2;
        fractal_raw_data.data[(2, 3)] = -5;
        fractal_raw_data.data[(1, 2)] = 3;

        {
            println!("\n\n ----  JSON  ----  \n\n");
            // JSON
            // Convert the FractalRawData to a JSON string.
            let serialized = serde_json::to_string(&fractal_raw_data).unwrap();

            // Prints serialized = {"angle_count":1,"rate_count":2}
            println!("serialized = {:?}", serialized);

            // Convert the JSON string back to a FractalRawData.
            let deserialized: FractalRawData = serde_json::from_str(&serialized).unwrap();

            // Prints deserialized = FractalRawData { angle_count: 1, rate_count: 2 }
            println!("deserialized = {:?}", deserialized);
        }

        {
            println!("\n\n ----  BINARY  ----  \n\n");
            // binary encoding
            let serialized: Vec<u8> = bincode::serialize(&fractal_raw_data).unwrap();
            print!("serialized: ");
            for num in serialized.iter() {
                print!("{},", num);
            }
            println!("Done!");
            let deserialized: FractalRawData = bincode::deserialize(&serialized[..]).unwrap();
            println!("deserialized = {:?}", deserialized);

            let filename = crate::file_io::build_output_path_with_date_time(vec!["ddp_utils"])
                .join("binary_encoding_test.dat")
                .to_owned();

            use std::io::prelude::*;

            // now write to disk
            {
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&filename)
                    .unwrap();

                file.write_all(&serialized[..]).unwrap();
            }
            // and read it back:
            {
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(false)
                    .create(false)
                    .open(&filename)
                    .unwrap();
                let mut deserialized_buffer = Vec::<u8>::new();
                file.read_to_end(&mut deserialized_buffer).unwrap();
                let deserialized_from_file: FractalRawData =
                    bincode::deserialize(&deserialized_buffer[..]).unwrap();
                println!("\ndeserialized_from_file = {:?}", deserialized_from_file);
            }
        }
    }
}
