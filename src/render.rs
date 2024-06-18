use rayon::prelude::{IntoParallelIterator, ParallelExtend, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageSpecification {
    pub resolution: nalgebra::Vector2<u32>,
    pub center: nalgebra::Vector2<f64>,
    pub width: f64,
}

/**
 * Used to fully-specify both an image resolution and how it is anchored into the "real"
 * space in which the fractal (or other subject) lives. The height in "real" space is derived
 * from the aspect ratio of the image and the specified width.
 */
impl ImageSpecification {
    // TODO:  add a unit test for this!!
    pub fn height(&self) -> f64 {
        self.width * (self.resolution[1] as f64) / (self.resolution[0] as f64)
    }

    /**
     * Used for anti-aliasing the image calculations. Computes a vector of offsets to be
     * applied within a single pixel, generating a dense grid of samples within that pixel.
     */
    // TODO:  add a unit test for this!!
    pub fn subpixel_offset_vector(&self, n: u32) -> Vec<nalgebra::Vector2<f64>> {
        let mut offsets = Vec::with_capacity((n * n) as usize);
        let step = 1.0 / n as f64;

        let pixel_width = self.width / (self.resolution[0] as f64);
        let pixel_height = self.height() / (self.resolution[1] as f64);

        for i in 0..n {
            let alpha_i = 0.5 - step * (i as f64); // [-0.5, 0.5)
            let x = alpha_i * pixel_width;

            for j in 0..n {
                let alpha_j = 0.5 - step * (j as f64); // [-0.5, 0.5)
                let y = alpha_j * pixel_height;
                offsets.push(nalgebra::Vector2::new(x, y));
            }
        }

        offsets
    }
}

/**
 * Allows the user to specify only the resolution of the image and how much "extra space" to leave
 * around the fractal (subject) in the image. The real coordinates are derived automatically from
 * other information in the fractal.
 *
 * The `FitImage` can then be converted into a full `ImageSpecification` to be passed into other code.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FitImage {
    pub resolution: nalgebra::Vector2<u32>,
    pub padding_scale: f64,
}

impl FitImage {
    pub fn image_specification(
        &self,
        dims: &nalgebra::Vector2<f64>,
        center: &nalgebra::Vector2<f64>,
    ) -> ImageSpecification {
        let pixel_height = self.resolution[1] as f64;
        let pixel_width = self.resolution[0] as f64;
        let dims_height = dims[1];
        let dims_width = dims[0];

        let aspect_ratio = pixel_height / pixel_width; // of the rendered image
        let selected_width = if aspect_ratio > (dims_height / dims_width) {
            dims_width
        } else {
            dims_height / aspect_ratio
        };

        ImageSpecification {
            resolution: self.resolution,
            center: *center,
            width: self.padding_scale * selected_width,
        }
    }
}

/**
 * Used to map from image space into the "regular" domain used to generate the fractals.
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

    // Map from pixel (integer) to point (float)
    pub fn map(&self, index: u32) -> f64 {
        self.offset + self.slope * (index as f64)
    }

    // Maps from point to pixel.
    // Rename as part of https://github.com/MatthewPeterKelly/fractal-renderer/issues/48?
    pub fn inverse_map(&self, point: f64) -> i32 {
        ((point - self.offset) / self.slope) as i32
    }
}

pub struct PixelMapper {
    width: LinearPixelMap,
    height: LinearPixelMap,
}

// TODO:  standardize on "point" = Vector2 and "pixel_coordinate" = (u32, u32)?
// https://github.com/MatthewPeterKelly/fractal-renderer/issues/47
// Improve this class generally:
// https://github.com/MatthewPeterKelly/fractal-renderer/issues/48
impl PixelMapper {
    pub fn new(image_specification: &ImageSpecification) -> PixelMapper {
        PixelMapper {
            width: LinearPixelMap::new_from_center_and_width(
                image_specification.resolution[0],
                image_specification.center[0],
                image_specification.width,
            ),
            height: LinearPixelMap::new_from_center_and_width(
                image_specification.resolution[1],
                image_specification.center[1],
                -image_specification.height(),
            ),
        }
    }

    pub fn inverse_map(&self, point: &nalgebra::Vector2<f64>) -> (i32, i32) {
        (
            self.width.inverse_map(point[0]),
            self.height.inverse_map(point[1]),
        )
    }
}

// Use the PixelMapper to map from "point" to "pixel" space, and then
// use existing utilitites in the ImageBuffer to draw the pixel at a specific color

/**
 * Small utility function that resets a stopwatch and returns the elapsed time.
 */
pub fn elapsed_and_reset(stopwatch: &mut Instant) -> Duration {
    let duration = stopwatch.elapsed();
    *stopwatch = Instant::now();
    duration
}

/**
 * Given image size parameters and a mapping into "regular" space used by the fractal,
 * iterate over each pixel, using a lambda function to compute the "value" of the fractal image
 * at each pixel location.
 *
 * This is used as the core implementation for pixel-based fractals, such as the Mandelbrot set and
 * the Driven-Damped Pendulum attractor.
 *
 * @param pixel_renderer:  maps from a point in the image (regular space, not pixels) to a scalar
 * value which can then later be plugged into a color map by the rendering pipeline.
 */
pub fn generate_scalar_image<F>(spec: &ImageSpecification, pixel_renderer: F) -> Vec<Vec<f64>>
where
    F: Fn(&nalgebra::Vector2<f64>) -> f64 + std::marker::Sync,
{
    let pixel_map_width =
        LinearPixelMap::new_from_center_and_width(spec.resolution[0], spec.center[0], spec.width);

    let pixel_map_height = LinearPixelMap::new_from_center_and_width(
        spec.resolution[1],
        spec.center[1],
        -spec.height(), // Image coordinates are upside down.
    );

    let mut raw_data: Vec<Vec<f64>> = Vec::with_capacity(spec.resolution[0] as usize);
    raw_data.par_extend((0..spec.resolution[0]).into_par_iter().map(|x| {
        let re = pixel_map_width.map(x);
        (0..spec.resolution[1])
            .map(|y| {
                let im = pixel_map_height.map(y);
                pixel_renderer(&nalgebra::Vector2::<f64>::new(re, im))
            })
            .collect()
    }));

    raw_data
}
