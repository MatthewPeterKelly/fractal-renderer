use numerical_methods;
use std::convert::TryInto;

fn main() {
    use nalgebra::DMatrix;
    use nalgebra::Vector2;
    use numerical_methods::ddp_utils::compute_basin_of_attraction;
    use numerical_methods::ddp_utils::FractalRawData;
    use numerical_methods::pixel_iter::PixelMap;
    use numerical_methods::pixel_iter::Point2d;

    let n_angle = 20; // TODO:  use `usize`?
    let n_rate = 20;
    let max_rate = 3.0;

    let verbose = true;

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
    }

    if verbose {
        println!("Fractal raw data:\n{:?}", fractal_raw_data);
    }
}
