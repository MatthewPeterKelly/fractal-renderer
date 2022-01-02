use numerical_methods;

fn main() {
    // use nalgebra::Vector2;
    use numerical_methods::pixel_iter::Point2d;
    use numerical_methods::pixel_iter::PixelMap;

    let n_angle = 10;
    let n_rate = 10;
    let max_rate = 3.0;


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

    // Populate the data for a single row
    for angle in 0..n_angle {
        for rate in 0..n_rate {
            let point = pixel_map.map(angle, rate);
            println!("Point: {:?}", point);
        }
    }

}