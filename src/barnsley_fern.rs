use rand::distributions::{Distribution, Uniform};

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::render;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BarnsleyFernParams {
    pub fit_image: render::FitImage,
    pub sample_count: u32,
    pub background_color_rgba: [u8; 4],
    pub fern_color_rgba: [u8; 4],
}

impl Default for BarnsleyFernParams {
    fn default() -> BarnsleyFernParams {
        BarnsleyFernParams {
            fit_image: render::FitImage::default(),
            sample_count: 1000,
            background_color_rgba: [0, 0, 0, 255],
            fern_color_rgba: [79, 121, 66, 255],
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

// x values: from -3 to 3
// y values: from 0 to 10
const FERN_CENTER: nalgebra::Vector2<f64> = nalgebra::Vector2::new(0.0, 5.0);
const FERN_HEIGHT: f64 = 10.0;
const FERN_WIDTH: f64 = 6.0;

pub fn barnsley_f1_update(prev: &nalgebra::Vector2<f64>) -> nalgebra::Vector2<f64> {
    nalgebra::Vector2::<f64>::new(0.0, 0.16 * prev[1])
}

pub fn barnsley_f2_update(prev: &nalgebra::Vector2<f64>) -> nalgebra::Vector2<f64> {
    const A: nalgebra::Matrix2<f64> = nalgebra::Matrix2::<f64>::new(0.85, 0.04, -0.04, 0.85);
    const B: nalgebra::Vector2<f64> = nalgebra::Vector2::<f64>::new(0.0, 1.60);
    A * prev + B
}

pub fn barnsley_f3_update(prev: &nalgebra::Vector2<f64>) -> nalgebra::Vector2<f64> {
    const A: nalgebra::Matrix2<f64> = nalgebra::Matrix2::<f64>::new(0.20, -0.26, 0.23, 0.22);
    const B: nalgebra::Vector2<f64> = nalgebra::Vector2::<f64>::new(0.0, 1.60);
    A * prev + B
}

pub fn barnsley_f4_update(prev: &nalgebra::Vector2<f64>) -> nalgebra::Vector2<f64> {
    const A: nalgebra::Matrix2<f64> = nalgebra::Matrix2::<f64>::new(-0.15, 0.28, 0.26, 0.24);
    const B: nalgebra::Vector2<f64> = nalgebra::Vector2::<f64>::new(0.0, 0.44);
    A * prev + B
}

// Fern Generation Algorithm taken from:
// https://en.wikipedia.org/wiki/Barnsley_fern

pub fn next_barnsley_fern_sample<R: Rng>(
    rng: &mut R,
    prev: &nalgebra::Vector2<f64>,
) -> nalgebra::Vector2<f64> {
    // TODO:  construct only once as part of https://github.com/MatthewPeterKelly/fractal-renderer/issues/46
    let distribution = Uniform::from(0.0..1.0);
    let sample = distribution.sample(rng);

    if sample < 0.85 {
        return barnsley_f2_update(prev);
    }
    if sample < 0.92 {
        return barnsley_f3_update(prev);
    }
    if sample < 0.99 {
        return barnsley_f4_update(prev);
    }
    barnsley_f1_update(prev)
}

pub fn render_barnsley_fern(
    params: &BarnsleyFernParams,
    directory_path: &std::path::Path,
    file_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch: Instant = Instant::now();
    let mut timer = MeasuredElapsedTime::default();

    // write out the parameters to a file:
    let params_path = directory_path.join(file_prefix.to_owned() + ".json");
    let params_str = serde_json::to_string(params)?;
    std::fs::write(params_path, params_str).expect("Unable to write params file.");

    let render_path = directory_path.join(file_prefix.to_owned() + ".png");

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::new(
        params.fit_image.resolution[0],
        params.fit_image.resolution[1],
    );

    let background_color = image::Rgba(params.background_color_rgba);
    let fern_color = image::Rgba(params.fern_color_rgba);

    // Set the background to black:
    for (_, _, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = background_color;
    }

    let image_specification = params.fit_image.image_specification(
        &nalgebra::Vector2::new(FERN_WIDTH, FERN_HEIGHT),
        &FERN_CENTER,
    );

    let pixel_mapper = render::PixelMapper::new(&image_specification);
    let mut sample_point = nalgebra::Vector2::<f64>::new(0.0, 0.0);

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    let mut rng = rand::thread_rng();

    for _ in 0..params.sample_count {
        sample_point = next_barnsley_fern_sample(&mut rng, &sample_point);
        let (x, y) = pixel_mapper.inverse_map(&sample_point);
        if let Some(pixel) = imgbuf.get_pixel_mut_checked(x as u32, y as u32) {
            *pixel = fern_color;
        }
    }

    timer.sampling = render::elapsed_and_reset(&mut stopwatch);

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
