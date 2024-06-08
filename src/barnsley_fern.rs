use rand::seq::index::sample;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::render;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BarnsleyFernParams {
    pub resolution: nalgebra::Vector2<u32>,
    pub sample_count: u32,
}

impl Default for BarnsleyFernParams {
    fn default() -> BarnsleyFernParams {
        BarnsleyFernParams {
            resolution: nalgebra::Vector2::<u32>::new(400, 300),
            sample_count: 1000,
        }
    }
}

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

const COLOR_BLACK: image::Rgb<u8> = image::Rgb([0, 0, 0]);
const COLOR_GREEN: image::Rgb<u8> = image::Rgb([79, 121, 66]);

// Fern Generation Algorithm taken from:
// https://en.wikipedia.org/wiki/Barnsley_fern

pub fn next_barnsley_fern_sample(prev: nalgebra::Vector2<f64>) -> nalgebra::Vector2<f64> {
    prev
}

pub fn render_barnsley_fern(
    params: &BarnsleyFernParams,
    directory_path: &std::path::Path,
    file_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    // TODO:  the following block could be a file I/O utility...

    // write out the parameters to a file:
    let params_path = directory_path.join(file_prefix.to_owned() + ".json");
    let params_str = serde_json::to_string(params)?;
    std::fs::write(params_path, params_str).expect("Unable to write params file.");

    let render_path = directory_path.join(file_prefix.to_owned() + ".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::new(
        params.resolution[0],
        params.resolution[1],
    );

    // Set the background to black:
    for (_, _, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = COLOR_BLACK;
    }

    let pixel_mapper = PixelMapper::new();
    let mut sample_point = nalgebra::Vector2::<f64>::new(0.0, 0.0);

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    for _ in 0..params.sample_count {
        sample_point = next_barnsley_fern_sample(sample_point);
        let pixel = pixel_mapper.inverse_map(sample_point);
        // TODO:  draw the pixel into the buffer!
        // canvas.draw_pixel(sample_point, COLOR_GREEN);
    }

    timer.sampling = render::elapsed_and_reset(&mut stopwatch);

    // TODO:  this terminal block of boilerplate could also be shared.

    // Save the image to a file, deducing the type from the file name
    imgbuf.save(&render_path).unwrap();
    timer.write_png = render::elapsed_and_reset(&mut stopwatch);

    println!("Wrote image file to: {}", render_path.display());

    timer.display(&mut crate::file_io::create_text_file(
        directory_path,
        file_prefix,
        "_diagnostics.txt",
    ))?;

    Ok(())
}
