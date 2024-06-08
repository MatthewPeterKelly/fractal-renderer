use rayon::prelude::{IntoParallelIterator, ParallelExtend, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageSpecification {
    pub resolution: nalgebra::Vector2<u32>,
    pub center: nalgebra::Vector2<f64>,
    pub width: f64,
}

impl ImageSpecification {
    pub fn height(&self) -> f64 {
        self.width * (self.resolution[1] as f64) / (self.resolution[0] as f64)
    }
}

impl Default for ImageSpecification {
    fn default() -> ImageSpecification {
        ImageSpecification {
            resolution: nalgebra::Vector2::<u32>::new(400, 300),
            center: nalgebra::Vector2::<f64>::new(0.0, 0.0),
            width: 1.0,
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
    // TODO:  use `i32` here (and fix consistency throughout)
    pub fn map(&self, index: u32) -> f64 {
        self.offset + self.slope * (index as f64)
    }

    //TODO:  add unit test for this...
    pub fn inverse_map(&self, point: f64) -> i32 {
        ((point - self.offset) / self.slope) as i32
    }
}

pub struct PixelMapper {
    width: LinearPixelMap,
    height: LinearPixelMap,
}

// TODO:  standardize on "point" = Vector2 and "index" = (u32, u32)?
// Logic: image library standardized on u32, u32
// need to do math on points
// Using a named type helps make things more consistent

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

    // TODO:  use this in other fractals
    // TODO: match naming convention for mapping direction... standardize it!
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
