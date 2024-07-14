use crate::core::image_utils::{FitImage, ViewRectangle};
use crate::{chaos_game, file_io};
use rand::distributions::{Distribution, Uniform};
use rand::Rng;
use serde::{Deserialize, Serialize};

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
    view_rectangle: ViewRectangle,

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
    pub fit_image: FitImage,
    pub sample_count: u32,
    pub subpixel_antialiasing: i32,
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
 */
pub fn render_barnsley_fern(
    params: &BarnsleyFernParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set up the "fern sample distribution":
    let mut sample_point = nalgebra::Vector2::<f64>::new(0.0, 0.0);
    let mut rng = rand::thread_rng();
    let generator = SampleGenerator::new(&params.coeffs);
    let fern_color = image::Rgba(params.fern_color_rgba);

    let mut distribution = || {
        sample_point = generator.next(&mut rng, &sample_point);
        chaos_game::ColoredPoint {
            point: sample_point,
            color: fern_color,
        }
    };

    chaos_game::render(
        image::Rgba(params.background_color_rgba),
        &mut distribution,
        params.sample_count,
        params.subpixel_antialiasing,
        &params
            .fit_image
            .image_specification(&params.coeffs.view_rectangle),
        file_prefix,
        &serde_json::to_string(params)?,
    )
}
