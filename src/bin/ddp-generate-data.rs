use chrono::{Datelike, Timelike, Utc};
use numerical_methods;
use std::convert::TryInto;

fn main() {
    use nalgebra::DMatrix;
    use nalgebra::Vector2;
    use numerical_methods::ddp_utils::compute_basin_of_attraction;
    use numerical_methods::ddp_utils::FractalRawData;
    use numerical_methods::pixel_iter::PixelMap;
    use numerical_methods::pixel_iter::Point2d;

    let now = Utc::now();
    let datetime = format!(
        "{}{}{}_{}{}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute()
    );
    let filename = "out/ddp_raw_data__".to_owned() + &datetime;

    // For sub-pixel anti-aliasing, pick a multiple of 120 = 2*3*4*5
    //////////////////////
    ////// BUG HERE //////
    //////////////////////
    // -->  it seems like image gets cut off if angle < rate count
    let n_angle = 10 * 480; // TODO:  use `usize`?
    let n_rate = 10 * 480;
    let max_rate = 12.0;

    let verbose = false;

    // Mapping between pixels and real values
    let pixel_map = PixelMap::new(
        Point2d {
            x: n_angle as f64,
            y: n_rate as f64,
        },
        Point2d { x: 0.0, y: 0.0 },
        Point2d {
            x: 2.0 * std::f64::consts::PI, // one period
            y: 2.0 * max_rate,
        },
    );

    // We're going to be populating this:
    // TODO: make a proper constructor for this...
    let mut fractal_raw_data = FractalRawData {
        angle_count: n_angle,
        rate_count: n_rate,
        max_rate: max_rate,
        data: DMatrix::from_element(n_angle.try_into().unwrap(), n_rate.try_into().unwrap(), 0),
    };

    // Populate the data for a single row
    for angle in 0..n_angle {
        for rate in 0..n_rate {
            let point = pixel_map.map(angle, rate);
            let x = Vector2::new(point.x, point.y);
            let x_idx = compute_basin_of_attraction(x);
            if let Some(index) = x_idx {
                fractal_raw_data.data[(angle as usize, rate as usize)] = index;
            } else {
                println!("Point: {:?} --> ERROR", point);
            }
        }
        println!("Angle: {} / {}", angle + 1, n_angle);
    }

    // TODO:  print stop time

    if verbose {
        println!("Fractal raw data:\n{:?}", fractal_raw_data);
    }

    // binary encoding
    let serialized: Vec<u8> = bincode::serialize(&fractal_raw_data).unwrap();

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
}
