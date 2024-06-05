use rayon::prelude::{IntoParallelIterator, ParallelExtend, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageSpecification {
    pub resolution: nalgebra::Vector2<u32>,
    pub center: nalgebra::Vector2<f64>,
    pub width: f64,
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

    pub fn map(&self, index: u32) -> f64 {
        self.offset + self.slope * (index as f64)
    }
}

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
    let pixel_map_real =
        LinearPixelMap::new_from_center_and_width(spec.resolution[0], spec.center[0], spec.width);
    let image_height = spec.width * (spec.resolution[1] as f64) / (spec.resolution[0] as f64);

    let pixel_map_imag = LinearPixelMap::new_from_center_and_width(
        spec.resolution[1],
        spec.center[1],
        -image_height, // Image coordinates are upside down.
    );

    let mut raw_data: Vec<Vec<f64>> = Vec::with_capacity(spec.resolution[0] as usize);
    raw_data.par_extend((0..spec.resolution[0]).into_par_iter().map(|x| {
        let re = pixel_map_real.map(x);
        (0..spec.resolution[1])
            .map(|y| {
                let im = pixel_map_imag.map(y);
                pixel_renderer(&nalgebra::Vector2::<f64>::new(re, im))
            })
            .collect()
    }));

    raw_data
}
