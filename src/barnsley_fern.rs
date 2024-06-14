use rand::distributions::{Distribution, Uniform};

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::{file_io, render};

// Fern Generation Algorithm reference:
// https://en.wikipedia.org/wiki/Barnsley_fern

/**
 * The Barnsley Fern is implemented by a sequence of samples, where each maps from the previous using a 2D affine transform. There are four possible transforms, which are selected randomly (with non-uniform weights).
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscreteMapCoeff {
    linear: nalgebra::Matrix2<f64>,
    offset: nalgebra::Vector2<f64>,
    weight: f64,
}

impl DiscreteMapCoeff {
    pub fn map(&self, prev: &nalgebra::Vector2<f64>) -> nalgebra::Vector2<f64> {
        self.linear * prev + self.offset
    }
}

/**
 * Coefficients needed to generate the Barnsley Fern fractal.
 * This is where the bulk of the "math" for the fractal occurs.
 *
 * This data structure is used to import all "parameters" from the JSON
 * file, specified by the user.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Coeffs {
    // x values: from -3 to 3
    // y values: from 0 to 10
    center: nalgebra::Vector2<f64>,
    dimensions: nalgebra::Vector2<f64>, // width, height

    f1_map: DiscreteMapCoeff,
    f2_map: DiscreteMapCoeff,
    f3_map: DiscreteMapCoeff,
    f4_map: DiscreteMapCoeff,
}

impl Coeffs {
    pub fn normalize_weights(&mut self) {
        let total =
            self.f1_map.weight + self.f2_map.weight + self.f3_map.weight + self.f4_map.weight;
        let scale = 1.0 / total;
        self.f1_map.weight *= scale;
        self.f2_map.weight *= scale;
        self.f3_map.weight *= scale;
        self.f4_map.weight *= scale;
    }
}

/**
 * Complete set of parameters that are fed in from the JSON for the Barnsley Fern fractal.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BarnsleyFernParams {
    pub fit_image: render::FitImage,
    pub sample_count: u32,
    pub background_color_rgba: [u8; 4],
    pub fern_color_rgba: [u8; 4],
    pub coeffs: Coeffs,
}

/**
 * Wrapper around `Coeffs`, used to precompute a few things before
 * running the sample generation.
 */
pub struct SampleGenerator {
    distribution: Uniform<f64>,
    f2_threshold: f64,
    f3_threshold: f64,
    f4_threshold: f64,
    coeffs: Coeffs,
}

impl SampleGenerator {
    pub fn new(raw_coeffs: &Coeffs) -> SampleGenerator {
        let mut coeffs = raw_coeffs.clone();
        coeffs.normalize_weights();

        SampleGenerator {
            distribution: Uniform::from(0.0..1.0),
            f2_threshold: coeffs.f2_map.weight,
            f3_threshold: coeffs.f2_map.weight + coeffs.f3_map.weight,
            f4_threshold: coeffs.f2_map.weight + coeffs.f3_map.weight + coeffs.f4_map.weight,
            coeffs,
        }
    }

    pub fn next<R: Rng>(
        &self,
        rng: &mut R,
        prev_sample: &nalgebra::Vector2<f64>,
    ) -> nalgebra::Vector2<f64> {
        let r = self.distribution.sample(rng);
        if r < self.f2_threshold {
            return self.coeffs.f2_map.map(prev_sample);
        }
        if r < self.f3_threshold {
            return self.coeffs.f3_map.map(prev_sample);
        }
        if r < self.f4_threshold {
            return self.coeffs.f4_map.map(prev_sample);
        }
        self.coeffs.f1_map.map(prev_sample)
    }
}

/**
 * Called by main, used to render the fractal using the above data structures.
 *
 * Note:  most of this code is agnostic to the Barnsley Fern. It could be pulled out into
 * a common library whenever the next sample-based fractal is added to the project.
 */
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

    // MPK
    let background_color = image::Rgba(params.background_color_rgba);
    let fern_color = image::Rgba(params.fern_color_rgba);

    for (_, _, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = background_color;
    }

    // MPK
    let image_specification = params
        .fit_image
        .image_specification(&params.coeffs.dimensions, &params.coeffs.center);

    let pixel_mapper = render::PixelMapper::new(&image_specification);

    // MPK
    let mut sample_point = nalgebra::Vector2::<f64>::new(0.0, 0.0);

    timer.setup = render::elapsed_and_reset(&mut stopwatch);

    let mut rng = rand::thread_rng();

    // MPK
    let generator = SampleGenerator::new(&params.coeffs);

    for _ in 0..params.sample_count {
        sample_point = generator.next(&mut rng, &sample_point);
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
