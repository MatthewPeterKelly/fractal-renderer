use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::{
    file_io,
    histogram::{CumulativeDistributionFunction, Histogram},
    render,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    pub image_specification: render::ImageSpecification,
    // Convergence criteria
    pub escape_radius_squared: f64,
    pub max_iter_count: u32,
    pub refinement_count: u32,
    pub histogram_bin_count: usize,
}

/**
 * @param dimensions: local "width" and "height" of the retangle in imaginary space
 * @param center: location of the center of that rectangle
 */
pub fn complex_range(
    dimensions: nalgebra::Vector2<f64>,
    center: nalgebra::Vector2<f64>,
) -> nalgebra::Vector2<std::ops::Range<f64>> {
    let real_range = (center[0] - 0.5 * dimensions[0])..(center[0] + 0.5 * dimensions[0]);
    let imag_range = (center[1] - 0.5 * dimensions[1])..(center[1] + 0.5 * dimensions[1]);
    nalgebra::Vector2::<std::ops::Range<f64>>::new(real_range, imag_range)
}

/**
 * Data structure for storing the internal state of the mandelbrot sequence calculation.
 * Highly optimized version of the equation to reduce floating point operation count.
 */
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
    fn new(point: &nalgebra::Vector2<f64>) -> MandelbrotSequence {
        let mut value = MandelbrotSequence {
            x0: point[0],
            y0: point[1],
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
        test_point: &nalgebra::Vector2<f64>,
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

#[derive(Default)]
pub struct MeasuredElapsedTime {
    pub setup: Duration,
    pub mandelbrot: Duration,
    pub histogram: Duration,
    pub cdf: Duration,
    pub color_map: Duration,
    pub write_png: Duration,
}

impl MeasuredElapsedTime {
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "MeasuredElapsedTime:")?;
        writeln!(writer, " -- Setup:      {:?}", self.setup)?;
        writeln!(writer, " -- Mandelbrot: {:?}", self.mandelbrot)?;
        writeln!(writer, " -- Histogram:  {:?}", self.histogram)?;
        writeln!(writer, " -- CDF:        {:?}", self.cdf)?;
        writeln!(writer, " -- Color Map:  {:?}", self.color_map)?;
        writeln!(writer, " -- Write PNG:  {:?}", self.write_png)?;
        writeln!(writer)?;
        Ok(())
    }
}

pub fn render_mandelbrot_set(
    params: &MandelbrotParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    let render_path = file_prefix.with_suffix(".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        params.image_specification.resolution[0],
        params.image_specification.resolution[1],
    );

    // write out the parameters too:
    let params_path = file_prefix.with_suffix(".json");
    std::fs::write(params_path, serde_json::to_string(params)?).expect("Unable to write file");

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    let pixel_renderer = |point: &nalgebra::Vector2<f64>| {
        let result = MandelbrotSequence::normalized_escape_count(
            point,
            params.escape_radius_squared,
            params.max_iter_count,
            params.refinement_count,
        );
        result.unwrap_or(0.0)
    };

    let raw_data = render::generate_scalar_image(&params.image_specification, pixel_renderer);

    timer.mandelbrot = render::elapsed_and_reset(&mut stopwatch);

    // Compute the histogram by iterating over the raw data.
    let mut hist = Histogram::new(params.histogram_bin_count, params.max_iter_count as f64);
    raw_data.iter().for_each(|row| {
        row.iter().for_each(|&val| {
            if val > 0.0 {
                hist.insert(val);
            }
        });
    });

    timer.histogram = render::elapsed_and_reset(&mut stopwatch);

    // Now compute the CDF from the histogram, which will allow us to normalize the color distribution
    let cdf = CumulativeDistributionFunction::new(&hist);

    timer.cdf = render::elapsed_and_reset(&mut stopwatch);

    // Iterate over the coordinates and pixels of the image
    let color_map = create_color_map_black_blue_white();
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = color_map(cdf.percentile(raw_data[x as usize][y as usize]));
    }

    timer.color_map = render::elapsed_and_reset(&mut stopwatch);

    // Save the image to a file, deducing the type from the file name
    imgbuf.save(&render_path).unwrap();
    timer.write_png = render::elapsed_and_reset(&mut stopwatch);

    println!("Wrote image file to: {}", render_path.display());

    let mut diagnostics_file = file_prefix.create_file_with_suffix("_diagnostics.txt");

    timer.display(&mut diagnostics_file)?;
    cdf.display(&mut diagnostics_file)?;
    hist.display(&mut diagnostics_file)?;

    Ok(())
}

fn create_color_map_black_blue_white() -> impl Fn(f64) -> image::Rgb<u8> {
    move |input: f64| {
        const THRESHOLD: f64 = 0.7;
        if input > THRESHOLD {
            let alpha = input - THRESHOLD;
            const SCALE: f64 = 255.0 / (1.0 - THRESHOLD);
            let x = (alpha * SCALE) as u8;
            image::Rgb([x, x, 255])
        } else {
            const SCALE: f64 = 255.0 / THRESHOLD;
            let alpha = input * SCALE;
            let x = alpha as u8;
            image::Rgb([0, 0, x])
        }
    }
}
