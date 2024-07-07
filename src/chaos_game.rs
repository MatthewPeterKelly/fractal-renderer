/**
 * This module is used to generate fractals using the "chaos game" method,
 * in which a discrete sequence of points is sampled, and rendering those
 * points will converge to some fractal.
 */
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use image::Pixel;

use crate::{file_io, histogram::Histogram, render};

/**
 * Timing data, used for simple analysis logging.
 */
#[derive(Default)]
pub struct MeasuredElapsedTime {
    pub setup: Duration,
    pub sampling: Duration,
    pub write_raw_png: Duration,
    pub antialiasing_post_process: Duration,
}

impl MeasuredElapsedTime {
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "MeasuredElapsedTime:")?;
        writeln!(writer, " -- Setup:      {:?}", self.setup)?;
        writeln!(writer, " -- Sampling: {:?}", self.sampling)?;
        writeln!(writer, " -- Write PNG:  {:?}", self.write_raw_png)?;
        writeln!(
            writer,
            " -- Antialiasing post-processing:  {:?}",
            self.antialiasing_post_process
        )?;
        writeln!(writer)?;
        Ok(())
    }
}

pub struct ColoredPoint {
    pub point: nalgebra::Vector2<f64>,
    pub color: image::Rgba<u8>,
}

/**
 * Renders a fractal defined by randomly generated sequence of points from a carefully crafted distribution.
 * The user sets up the distribution, and this function samples from the distribution and handles all of the
 * file generation and diagnostics.
 */
pub fn render<D>(
    background_color: image::Rgba<u8>,
    distribution_generator: &mut D,
    sample_count: u32,
    subpixel_antialiasing: i32,
    image_specification: &render::ImageSpecification,
    file_prefix: &file_io::FilePrefix,
    params_str: &str, // For diagnostics only --> written to a file
) -> Result<(), Box<dyn std::error::Error>>
where
    D: FnMut() -> ColoredPoint,
{
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    // write out the parameters to a file:
    let params_path = file_prefix.with_suffix(".json");
    std::fs::write(params_path, params_str).expect("Unable to write params file.");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::new(
        image_specification.resolution[0],
        image_specification.resolution[1],
    );

    // Create a second buffer in which to store the antialiasing mask:
    let mut subpixel_mask = nalgebra::DMatrix::from_element(
        image_specification.resolution[0] as usize,
        image_specification.resolution[1] as usize,
        render::SubpixelGridMask::new(),
    );

    for (_, _, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = background_color;
    }

    let pixel_mapper =
        render::UpsampledPixelMapper::new(image_specification, subpixel_antialiasing);

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    for _ in 0..sample_count {
        let colored_point = distribution_generator();
        let index = pixel_mapper.inverse_map(&colored_point.point);
        let (x, y) = index.pixel;

        if let Some(pixel) = imgbuf.get_pixel_mut_checked(x as u32, y as u32) {
            *pixel = colored_point.color;
            subpixel_mask[(x as usize, y as usize)].insert(subpixel_antialiasing, index.subpixel)
        }
    }

    timer.sampling = render::elapsed_and_reset(&mut stopwatch);

    // Save the image to a file, deducing the type from the file name
    let raw_render_path = file_prefix.with_suffix("_raw.png");
    imgbuf.save(&raw_render_path).unwrap();
    timer.write_raw_png = render::elapsed_and_reset(&mut stopwatch);
    println!("Wrote raw image file to: {}", raw_render_path.display());

    // Scale back the colors toward the background, based on the subpixel sample data:
    let antialiasing_scale = 1.0 / ((subpixel_antialiasing * subpixel_antialiasing) as f32);

    let mut histogram = Histogram::new(
        (subpixel_antialiasing * subpixel_antialiasing + 1) as usize,
        1.5,
    );

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let weight_low =
            antialiasing_scale * (subpixel_mask[(x as usize, y as usize)].count_ones() as f32);
        let weight_upp = 1.0 - weight_low;
        pixel.apply2(&background_color, |low: u8, upp: u8| -> u8 {
            ((low as f32) * weight_low + (upp as f32) * weight_upp) as u8
        });
        histogram.insert(weight_upp as f64);
    }
    timer.antialiasing_post_process = render::elapsed_and_reset(&mut stopwatch);

    // Save the antialiased image to a file, deducing the type from the file name
    let render_path = file_prefix.with_suffix(".png");
    imgbuf.save(&render_path).unwrap();
    timer.write_raw_png = render::elapsed_and_reset(&mut stopwatch);
    println!("Wrote image file to: {}", render_path.display());

    let mut diagnostics_file = file_prefix.create_file_with_suffix("_diagnostics.txt");
    timer.display(&mut diagnostics_file)?;
    histogram.display(&mut diagnostics_file)?;

    Ok(())
}
