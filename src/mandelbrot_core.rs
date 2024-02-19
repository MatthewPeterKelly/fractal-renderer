use rayon::prelude::{IntoParallelIterator, ParallelExtend, ParallelIterator};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::histogram::{CumulativeDistributionFunction, Histogram};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    // Where to render?
    pub image_resolution: nalgebra::Complex<u32>,
    pub center: nalgebra::Complex<f64>,
    pub view_scale_real: f64,
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
            view_scale_real: (3.0),
            escape_radius_squared: (4.0),
            max_iter_count: (550),
            refinement_count: (5),
        }
    }
}
impl MandelbrotParams {
    pub fn view_scale_im(&self) -> f64 {
        self.view_scale_real * (self.image_resolution.im as f64) / (self.image_resolution.re as f64)
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

/**
 * Used to map from image space into the complex domain.
 */
pub struct LinearPixelMap {
    offset: f64,
    slope: f64,
}

impl LinearPixelMap {
    /**
     * @param n: number of pixels spanned by [x0,x1]
     * @param x0: output of the map at 0
     * @param x1: output of the map at n-1
     */
    pub fn new(n: u32, x0: f64, x1: f64) -> LinearPixelMap {
        assert!(n > 0);
        let offset = x0;
        let slope = (x1 - x0) / ((n - 1) as f64);
        LinearPixelMap { offset, slope }
    }

    pub fn new_from_center_and_width(n: u32, center: f64, width: f64) -> LinearPixelMap {
        LinearPixelMap::new(n, center - 0.5 * width, center + 0.5 * width)
    }

    pub fn map(&self, index: u32) -> f64 {
        self.offset + self.slope * (index as f64)
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

pub fn elapsed_and_reset(stopwatch: &mut Instant) -> Duration {
    let duration = stopwatch.elapsed();
    *stopwatch = Instant::now();
    duration
}

pub fn render_mandelbrot_set(
    params: &MandelbrotParams,
    directory_path: &std::path::Path,
    file_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    let render_path = directory_path.join(file_prefix.to_owned() + ".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf =
        image::ImageBuffer::new(params.image_resolution.re, params.image_resolution.im);

    // write out the parameters too:
    let params_path = directory_path.join(file_prefix.to_owned() + ".json");
    std::fs::write(params_path, serde_json::to_string(params)?).expect("Unable to write file");

    // Mapping from image space to complex space
    let pixel_map_real = LinearPixelMap::new_from_center_and_width(
        params.image_resolution.re,
        params.center.re,
        params.view_scale_real,
    );
    let pixel_map_imag = LinearPixelMap::new_from_center_and_width(
        params.image_resolution.im,
        params.center.im,
        -params.view_scale_im(), // Image coordinates are upside down.
    );

    timer.setup = elapsed_and_reset(&mut stopwatch);

    // Generate the raw data for the fractal, using Rayon to parallelize the calculation.
    let mut raw_data: Vec<Vec<f64>> = Vec::with_capacity(params.image_resolution.re as usize);
    raw_data.par_extend((0..params.image_resolution.re).into_par_iter().map(|x| {
        let re = pixel_map_real.map(x);
        (0..params.image_resolution.im)
            .map(|y| {
                let im = pixel_map_imag.map(y);
                let result = MandelbrotSequence::normalized_escape_count(
                    &nalgebra::Complex::<f64>::new(re, im),
                    params.escape_radius_squared,
                    params.max_iter_count,
                    params.refinement_count,
                );
                result.unwrap_or(0.0)
            })
            .collect()
    }));

    timer.mandelbrot = elapsed_and_reset(&mut stopwatch);

    // Compute the histogram by iterating over the raw data.
    let mut hist = Histogram::new(64, params.max_iter_count as f64);
    raw_data.iter().for_each(|row| {
        row.iter().for_each(|&val| {
            hist.insert(val);
        });
    });

    timer.histogram = elapsed_and_reset(&mut stopwatch);

    // Now compute the CDF from the histogram, which will allow us to normalize the color distribution
    let cdf = CumulativeDistributionFunction::new(&hist);

    timer.cdf = elapsed_and_reset(&mut stopwatch);

    // Iterate over the coordinates and pixels of the image
    let color_map = create_color_map_black_blue_white();
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = color_map(cdf.percentile(raw_data[x as usize][y as usize]));
    }

    timer.color_map = elapsed_and_reset(&mut stopwatch);

    // Save the image to a file, deducing the type from the file name
    imgbuf.save(&render_path).unwrap();
    timer.write_png = elapsed_and_reset(&mut stopwatch);

    let file =
        std::fs::File::create(directory_path.join(file_prefix.to_owned() + "_diagnostics.txt"))
            .expect("failed to create diagnostics file");
    let mut diagnostics_file = std::io::BufWriter::new(file);
    timer.display(&mut diagnostics_file)?;
    hist.display(&mut diagnostics_file)?;

    Ok(())
}

// fn create_grayscale_color_map() -> impl Fn(f64) -> image::Rgb<u8> {
//     use splines::{Interpolation, Key, Spline};

//     let spline = Spline::from_vec(vec![
//         Key::new(0.0, 0.0, Interpolation::Linear),
//         Key::new(0.5, 5.0, Interpolation::Linear),
//         Key::new(0.8, 30.0, Interpolation::Linear),
//         Key::new(0.96, 90.0, Interpolation::Linear),
//         Key::new(1.0, 255.0, Interpolation::Linear),
//     ]);

//     move |input: f64| {
//         let output = spline.sample(input).unwrap();
//         let output_u8 = output as u8;
//         image::Rgb([output_u8, output_u8, output_u8])
//     }
// }

// fn create_double_hsv_color_map() -> impl Fn(f64) -> image::Rgb<u8> {
//     move |input: f64| {
//         let mut hue = 2.0 * input;
//         if hue > 1.0 {
//             hue -= 1.0;
//         }
//         hue *= 360.0;
//         let sat = input;
//         let value = input;
//         let (r, g, b) = hsv::hsv_to_rgb(hue, sat, value);
//         image::Rgb([r, g, b])
//     }
// }

fn create_color_map_black_blue_white() -> impl Fn(f64) -> image::Rgb<u8> {
    move |input: f64| {
        let mut alpha = 2.0 * input;
        if alpha > 1.0 {
            alpha -= 1.0;
            let x = (255.0 * alpha) as u8;
            image::Rgb([x, x, 255])
        } else {
            let x = (255.0 * alpha) as u8;
            image::Rgb([0, 0, x])
        }
    }
}
