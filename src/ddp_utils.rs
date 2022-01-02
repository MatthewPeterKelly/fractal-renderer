//! Explicit ODE solvers

extern crate nalgebra as na;
use crate::ode_solvers::midpoint_simulate; // HACK
use serde::{Deserialize, Serialize};

// TODO:  add constructor for this!!!
#[derive(Serialize, Deserialize, Debug)]
pub struct FractalRawData {
    pub angle_count: u32, // note: duplicates matrix dimensions...
    pub rate_count: u32,  // note: duplicates matrix dimensions...
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
        assert_relative_eq!(crate::ddp_utils::driven_damped_pendulum_dynamics(0.0, x), y);
    }

    #[test]
    fn basin_attraction_test() {
        use crate::ddp_utils::driven_damped_pendulum_attractor;
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
        use crate::ddp_utils::driven_damped_pendulum_attractor;
        use crate::ddp_utils::midpoint_simulate;
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
        use crate::ddp_utils::compute_basin_of_attraction;
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

    // TODO:  use binary encoding:  https://crates.io/crates/bincode

    #[test]
    fn basic_serialization_demo() {
        use crate::ddp_utils::FractalRawData;
        use nalgebra::DMatrix;
        let mut fractal_raw_data = FractalRawData {
            angle_count: 10,
            rate_count: 20,
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

            let filename = "out/binary_test_data";
            use std::io::prelude::*;

            // now write to disk
            {
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(filename)
                    .unwrap();

                file.write_all(&serialized[..]).unwrap();
            }
            // and read it back:
            {
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(false)
                    .create(false)
                    .open(filename)
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
