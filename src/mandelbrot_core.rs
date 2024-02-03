use iter_num_tools::grid_space;
use nalgebra::Complex;
use plotters::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    // Where to render?
    pub image_resolution: nalgebra::Complex<u32>,
    pub center: nalgebra::Complex<f64>,
    pub extent_real: f64,
    // Convergence criteria
    pub escape_radius_squared: f64,
    pub max_iter_count: u32,
    pub refinement_count: u32,
}

impl Default for MandelbrotParams {
    fn default() -> MandelbrotParams {
        MandelbrotParams {
            image_resolution: nalgebra::Complex::<u32>::new(1920, 1080),
            center: nalgebra::Complex::<f64>::new(-0.2, 0.0),
            extent_real: (3.0),
            escape_radius_squared: (4.0),
            max_iter_count: (550),
            refinement_count: (5),
        }
    }
}

/**
 * @param dimensions: local "width" and "height" of the retangle in imaginary space
 * @param center: location of the center of that rectangle
 */
pub fn complex_range(
    dimensions: nalgebra::Complex<f64>,
    center: nalgebra::Complex<f64>,
) -> nalgebra::Complex<std::ops::Range<f64>> {
    let real_range = (center.re - 0.5 * dimensions.re)..(center.re + 0.5 * dimensions.re);
    let imag_range = (center.im - 0.5 * dimensions.im)..(center.im + 0.5 * dimensions.im);
    nalgebra::Complex::<std::ops::Range<f64>>::new(real_range, imag_range)
}

impl MandelbrotParams {
    /**
     * @return: range of the image specified by the paramters, in complex space.
     */
    fn complex_range(&self) -> nalgebra::Complex<std::ops::Range<f64>> {
        complex_range(
            nalgebra::Complex::<f64>::new(
                self.extent_real,
                self.extent_real * (self.image_resolution.im as f64)
                    / (self.image_resolution.re as f64),
            ),
            self.center,
        )
    }
}

pub struct MandelbrotSequence {
    pub x0: f64,
    pub y0: f64,
    pub x_sqr: f64,
    pub y_sqr: f64,
    pub x: f64,
    pub y: f64,
    pub iter_count: u32,
}

impl MandelbrotSequence {
    fn new(point: &nalgebra::Complex<f64>) -> MandelbrotSequence {
        let mut value = MandelbrotSequence {
            x0: point.re,
            y0: point.im,
            x_sqr: 0.0,
            y_sqr: 0.0,
            x: 0.0,
            y: 0.0,
            iter_count: 0,
        };
        value.step(); // ensures that cached values are correct
        value
    }

    fn radius_squared(&self) -> f64 {
        self.x_sqr + self.y_sqr
    }

    fn radius(&self) -> f64 {
        self.radius_squared().sqrt()
    }

    // Z = Z*Z + C
    fn step(&mut self) {
        self.y = (self.x + self.x) * self.y + self.y0;
        self.x = self.x_sqr - self.y_sqr + self.x0;
        self.x_sqr = self.x * self.x;
        self.y_sqr = self.y * self.y;
        self.iter_count += 1;
    }

    // @return: true -- escaped! false --> did not escape
    // @return: iteration count if the point escapes, otherwise None().
    fn step_until_condition(
        &mut self,
        max_iter_count: u32,
        max_radius_squared: f64,
    ) -> Option<f64> {
        while self.iter_count < max_iter_count {
            if self.radius_squared() > max_radius_squared {
                return Some(self.iter_count as f64);
            }
            self.step();
        }
        None
    }

    /**
     * @return: normalized iteration count (if escaped), or unset optional.
     */
    fn compute_normalized_escape(
        &mut self,
        max_iter_count: u32,
        max_radius_squared: f64,
        refinement_count: u32,
    ) -> Option<f64> {
        use std::f64;
        let _ = self.step_until_condition(max_iter_count, max_radius_squared);
        for _ in 0..refinement_count {
            self.step();
        }
        const SCALE: f64 = 1.0 / std::f64::consts::LN_2;
        let normalized_iteration_count =
            (self.iter_count as f64) - f64::ln(f64::ln(self.radius())) * SCALE;

        if normalized_iteration_count < max_iter_count as f64 {
            Some(normalized_iteration_count)
        } else {
            None
        }
    }

    /// Test whether a point is in the mandelbrot set.
    /// @param test_point: a point in the complex plane to test
    /// @param escape_radius_squared: a point is not in the mandelbrot set if it exceeds this radius squared from the origin during the mandelbrot iteration sequence.
    /// @param max_iter_count: assume that a point is in the mandelbrot set if this number of iterations is reached without exceeding the escape radius.
    /// @param refinement_count: normalize the escape count, providing smooth interpolation between integer "escape count" values.
    /// @return: normalized (smooth) iteration count if the point escapes, otherwise None().
    pub fn normalized_escape_count(
        test_point: &nalgebra::Complex<f64>,
        escape_radius_squared: f64,
        max_iter_count: u32,
        refinement_count: u32,
    ) -> Option<f64> {
        let mut escape_sequence = MandelbrotSequence::new(test_point);

        if refinement_count == 0 {
            return escape_sequence.step_until_condition(max_iter_count, escape_radius_squared);
        }

        escape_sequence.compute_normalized_escape(
            max_iter_count,
            escape_radius_squared,
            refinement_count,
        )
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
    std::fs::write(params_path, serde_json::to_string(params)?).expect("Unable to write file");

    root.fill(&BLACK)?;

    let range = params.complex_range();

    let grid_iterator = grid_space(
        [range.re.start, range.im.start]..=[range.re.end, range.im.end],
        [
            params.image_resolution.re as usize,
            params.image_resolution.im as usize,
        ],
    );

    let chart = ChartBuilder::on(&root).build_cartesian_2d(range.re, range.im)?;
    let plotting_area = chart.plotting_area();
    let color_map = create_grayscale_color_map(params.max_iter_count);

    for [point_re, point_im] in grid_iterator {
        let test_point = Complex::<f64>::new(point_re, point_im);
        let result = MandelbrotSequence::normalized_escape_count(
            &test_point,
            params.escape_radius_squared,
            params.max_iter_count,
            params.refinement_count,
        );
        if let Some(iter) = result {
            plotting_area.draw_pixel((point_re, point_im), &color_map(iter))?;
        } else {
            // Nothing -- we already colored this one with the default color at startup.
        }
    }

    // To avoid the IO failure being ignored silently, we manually call the present function
    root.present().expect("Unable to write result to file, please make sure 'plotters-doc-data' dir exists under current dir");
    println!("Result has been saved to {}", render_path.display());

    Ok(())
}

fn create_grayscale_color_map(max_iter_count: u32) -> impl Fn(f64) -> RGBColor {
    use splines::{Interpolation, Key, Spline};

    let max_input = (max_iter_count as f64).sqrt();
    let max_output = 255.0;

    let low = Key::new(0.0, 0.0, Interpolation::Linear);
    let mid = Key::new(0.2 * max_input, 0.05 * max_output, Interpolation::Linear);
    let upp = Key::new(max_input, max_output, Interpolation::Linear);
    let spline = Spline::from_vec(vec![low, mid, upp]);

    move |iter_count: f64| {
        let input = iter_count.sqrt();
        let output = spline.sample(input).unwrap();
        let output_u8 = output as u8;
        RGBColor(output_u8, output_u8, output_u8)
    }
}
