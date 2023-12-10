use iter_num_tools::grid_space;
use nalgebra::Complex;
use plotters::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    // Where to render?
    pub image_resolution: nalgebra::Complex<u32>,
    pub center: nalgebra::Complex<f64>,
    pub domain_real: f64,
    // Convergence criteria
    pub escape_radius_squared: f64,
    pub max_iter_count: u32,
}

impl Default for MandelbrotParams {
    fn default() -> MandelbrotParams {
        MandelbrotParams {
            image_resolution: nalgebra::Complex::<u32>::new(1920, 1080),
            center: nalgebra::Complex::<f64>::new(-0.2, 0.0),
            domain_real: (3.0),
            escape_radius_squared: (4.0),
            max_iter_count: (550),
        }
    }
}

#[derive(Debug)]
pub struct MandelbrotEscapeResult {
    // Initial query point
    pub point: nalgebra::Complex<f64>,

    // Iteration at escape, or maximum iteration
    pub iter_count: u32,

    // Radius squared at escape, or unset
    pub radius_sqr: Option<f64>,
}

/// Test whether a point is in the mandelbrot set.
/// @param test_point: a point in the complex plane to test
/// @param escape_radius_squared: a point is not in the mandelbrot set if it exceeds this radius squared from the origin during the mandelbrot iteration sequence.
/// @param max_iter_count: assume that a point is in the mandelbrot set if this number of iterations is reached without exceeding the escape radius.
/// @return: a `MandelbrotIterationResult` indicating whether the point is in the set, along with some diagnostics information (useful for drawing and analysis).
pub fn mandelbrot_iteration_count(
    test_point: &nalgebra::Complex<f64>,
    escape_radius_squared: f64,
    max_iter_count: u32,
) -> MandelbrotEscapeResult {
    let x0 = test_point.re;
    let y0 = test_point.im;
    // Optimized escape time iteration algorithm taken from Wikipedia:
    // https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set
    let mut x_sqr = 0.0;
    let mut y_sqr = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;

    for iter in 0..max_iter_count {
        y = (x + x) * y + y0;
        x = x_sqr - y_sqr + x0;
        x_sqr = x * x;
        y_sqr = y * y;
        if x_sqr + y_sqr > escape_radius_squared {
            return MandelbrotEscapeResult {
                point: *test_point,
                iter_count: iter,
                radius_sqr: Option::Some(x_sqr + y_sqr),
            };
        }
    }
    MandelbrotEscapeResult {
        point: *test_point,
        iter_count: max_iter_count,
        radius_sqr: Option::None,
    }
}

pub fn render_mandelbrot_set(
    params: &MandelbrotParams,
    directory_path: &std::path::Path,
    file_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let render_path = directory_path.join(file_prefix.to_owned() + ".png");

    let root = BitMapBackend::new(
        &render_path,
        (params.image_resolution.re, params.image_resolution.im),
    )
    .into_drawing_area();

    // write out the parameters too:
    let params_path = directory_path.join(file_prefix.to_owned() + ".json");
    std::fs::write(&params_path, serde_json::to_string(params)?).expect("Unable to write file");

    root.fill(&BLACK)?;

    let domain_imag = params.domain_real * (params.image_resolution.im as f64)
        / (params.image_resolution.re as f64);

    let real_range = (params.center.re - 0.5 * params.domain_real)
        ..(params.center.re + 0.5 * params.domain_real);

    let imag_range = (params.center.im - 0.5 * domain_imag)..(params.center.im + 0.5 * domain_imag);

    let grid_iterator = grid_space(
        [real_range.start, imag_range.start]..=[real_range.end, imag_range.end],
        [
            params.image_resolution.re as usize,
            params.image_resolution.im as usize,
        ],
    );

    let chart = ChartBuilder::on(&root).build_cartesian_2d(real_range, imag_range)?;

    let plotting_area = chart.plotting_area();

    let color_map = create_grayscale_color_map(params.max_iter_count);

    for [point_re, point_im] in grid_iterator {
        let result = mandelbrot_iteration_count(
            &Complex::<f64>::new(point_re, point_im),
            params.escape_radius_squared,
            params.max_iter_count,
        );

        if result.radius_sqr.is_some() {
            // This is a bit silly: we should draw at the pixel coordinate, not the image one.
            // we do an extra "up and back" and risk getting the wrong pixel due to aliasing.
            // If we keep this, we'll need to get the mapping to complex coordinates directly so
            // that it is 1:1...
            //
            // TODO:  fancy color interpolation stuff here (see Wikipedia).
            //
            // TODO: also, probably make a better color map...
            plotting_area.draw_pixel((point_re, point_im), &color_map(result.iter_count))?;
            // plotting_area.draw_pixel((point_re, point_im), &ViridisRGB::get_color(color_val))?;
        }
    }

    // To avoid the IO failure being ignored silently, we manually call the present function
    root.present().expect("Unable to write result to file, please make sure 'plotters-doc-data' dir exists under current dir");
    println!("Result has been saved to {}", render_path.display());

    Ok(())
}

fn create_grayscale_color_map(max_iter_count: u32) -> impl Fn(u32) -> RGBColor {
    use splines::{Interpolation, Key, Spline};

    let max_input = (max_iter_count as f64).sqrt();
    let max_output = 255.0;

    let low = Key::new(0.0, 0.0, Interpolation::Cosine);
    let upp = Key::new(max_input, max_output, Interpolation::default());
    let spline = Spline::from_vec(vec![low, upp]);

    move |iter_count: u32| {
        let input = (iter_count as f64).sqrt();
        let output = spline.sample(input).unwrap();
        let output_u8 = output as u8;
        RGBColor(output_u8, output_u8, output_u8)
    }
}
