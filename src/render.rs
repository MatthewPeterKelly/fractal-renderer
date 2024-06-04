use std::time::{Duration, Instant};

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
