/**
 * This module is used to generate fractals using the "chaos game" method,
 * in which a discrete sequence of points is sampled, and rendering those
 * points will converge to some fractal.
 */
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::{file_io, render};

/**
 * Timing data, used for simple analysis logging.
 */
#[derive(Default)]
pub struct MeasuredElapsedTime {
    pub setup: Duration,
    pub sampling: Duration,
    pub write_png: Duration,
}

impl MeasuredElapsedTime {
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "MeasuredElapsedTime:")?;
        writeln!(writer, " -- Setup:      {:?}", self.setup)?;
        writeln!(writer, " -- Sampling: {:?}", self.sampling)?;
        writeln!(writer, " -- Write PNG:  {:?}", self.write_png)?;
        writeln!(writer)?;
        Ok(())
    }
}

pub struct ColoredPoint {
    pub point: nalgebra::Vector2<f64>,
    pub color: image::Rgba<u8>,
}

/**
 * Renders a fractal defined by a
 */
pub fn render<D>(
    background_color: image::Rgba<u8>,
    distribution_generator: &D,
    sample_count: u32,
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

    let render_path = file_prefix.with_suffix(".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::new(
        image_specification.resolution[0],
        image_specification.resolution[1],
    );

    for (_, _, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = background_color;
    }

    let pixel_mapper = render::PixelMapper::new(&image_specification);

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    for _ in 0..sample_count {
        let colored_point = distribution_generator();
        let (x, y) = pixel_mapper.inverse_map(&colored_point.point);
        if let Some(pixel) = imgbuf.get_pixel_mut_checked(x as u32, y as u32) {
            *pixel = colored_point.color;
        }
    }

    // NOTE:  when we add anti-aliasing, just insert it near here, as a second parallel data strucutre that stores
    // The count per pixel. Then, in post processing, iterate over the image and interpolate between the stored
    // and background color. That allows for more than one subject color and also anti-aliasing.
    timer.sampling = render::elapsed_and_reset(&mut stopwatch);

    // Save the image to a file, deducing the type from the file name
    imgbuf.save(&render_path).unwrap();
    timer.write_png = render::elapsed_and_reset(&mut stopwatch);

    println!("Wrote image file to: {}", render_path.display());

    timer.display(&mut file_prefix.create_file_with_suffix("_diagnostics.txt"))?;

    Ok(())
}
