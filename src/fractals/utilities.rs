// This module contains utility functions for fractal generation
// that are used by multiple fractals and depend on multiple `core` modules.

use std::sync::Arc;

use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::core::{
    histogram::Histogram,
    image_utils::{ImageSpecification, PixelMapper},
};

/// Populates a histogram by uniformly sampling `sample_count` points in the
/// area defined by `image_specification` on a uniform grid. The sampling is
/// done in parallel, using the user-provided `query` function to evaluate
/// each point. Only points for which `query` returns `Some(value)` are
/// inserted into the histogram.
pub fn populate_histogram<F>(
    query: &F,
    image_specification: &ImageSpecification,
    sample_count: u32,
    histogram: Arc<Histogram>,
) where
    F: Fn(&[f64; 2]) -> Option<f32> + Sync,
{
    let hist_image_spec = image_specification.scale_to_total_pixel_count(sample_count);

    let pixel_mapper = PixelMapper::new(&hist_image_spec);

    (0..hist_image_spec.resolution[0])
        .into_par_iter()
        .for_each(|i| {
            let x = pixel_mapper.width.map(i);
            for j in 0..hist_image_spec.resolution[1] {
                let y = pixel_mapper.height.map(j);
                if let Some(value) = query(&[x, y]) {
                    histogram.insert(value);
                }
            }
        });
}
