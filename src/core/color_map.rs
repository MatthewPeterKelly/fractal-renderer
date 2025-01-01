use image::Rgb;
use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

use super::lookup_table::LookupTable;

/**
 * Represents a single "keyframe" of the color map, pairing a
 * "query" with the color that should be produced at that query point.
 */
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ColorMapKeyFrame {
    pub query: f32,       // specify location of this color within the map; on [0,1]
    pub rgb_raw: [u8; 3], // [R, G, B]
}

pub trait ColorMapper {
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8>;
}
pub trait Interpolator {
    fn interpolate(
        &self,
        query: f32,
        value_zero: &Vector3<f32>,
        value_one: &Vector3<f32>,
    ) -> Vector3<f32>;
}

/**
 * Simple implementation of a "piecewise linear" color map, where the colors
 * are represented by simple linear interpolation in RGB color space. This is
 * not "strictly correct" from a color standpoint, but it works well enough in
 * practice. For details see:
 * - https://github.com/MatthewPeterKelly/fractal-renderer/pull/71
 * - https://docs.rs/palette/latest/palette/
 */
#[derive(Default)]
pub struct ColorMap<F: Interpolator> {
    queries: Vec<f32>,
    rgb_colors: Vec<Vector3<f32>>, // [0,255], but as f32
    interpolator: F,
}

impl<F: Interpolator> ColorMap<F> {
    /**
     * Create a color map from a vector of keyframes. The queries must be
     *
     * monotonically increasing, and the first keyframe query must be zero
     * and the last keyframe query must be one. Colors are specified in RGB
     * space as `u8` values on [0,255].
     */
    pub fn new(keyframes: &Vec<ColorMapKeyFrame>, interpolator: F) -> ColorMap<F> {
        if keyframes.is_empty() {
            println!("ERROR:  keyframes are empty!");
            panic!();
        }
        if keyframes.first().unwrap().query != 0.0 {
            println!("ERROR:  initial keyframe query point must be 0.0!");
            panic!();
        }
        if keyframes.last().unwrap().query != 1.0 {
            println!("ERROR:  final keyframe query point must be 1.0!");
            panic!();
        }
        for i in 0..(keyframes.len() - 1) {
            if keyframes[i].query >= keyframes[i + 1].query {
                println!("ERROR:  keyframes should be monotonic, but are not!");
                panic!();
            }
        }

        let mut queries = Vec::with_capacity(keyframes.len());
        let mut rgb_colors = Vec::with_capacity(keyframes.len());

        for keyframe in keyframes {
            queries.push(keyframe.query);
            rgb_colors.push(Vector3::new(
                keyframe.rgb_raw[0] as f32,
                keyframe.rgb_raw[1] as f32,
                keyframe.rgb_raw[2] as f32,
            ));
        }

        ColorMap {
            queries,
            rgb_colors,
            interpolator,
        }
    }

    pub fn compute_pixel(&self, query: f32) -> image::Rgb<u8> {
        let color_rgb = self.compute_raw(query);
        image::Rgb([color_rgb[0] as u8, color_rgb[1] as u8, color_rgb[2] as u8])
    }

    /**
     * Evaluates the color map, modestly efficient for small numbers of
     * keyframes. Any query outside of [0,1] will be clamped.
     */
    fn compute_raw(&self, query: f32) -> Vector3<f32> {
        if query <= 0.0f32 {
            *self.rgb_colors.first().unwrap()
        } else if query >= 1.0f32 {
            *self.rgb_colors.last().unwrap()
        } else {
            let idx_upp = self
                .queries
                .partition_point(|test_query| query >= *test_query);
            let idx_low = idx_upp - 1;
            let alpha =
                (query - self.queries[idx_low]) / (self.queries[idx_upp] - self.queries[idx_low]);
            self.interpolator.interpolate(
                alpha,
                &self.rgb_colors[idx_low],
                &self.rgb_colors[idx_upp],
            )
        }
    }
}

impl<F> ColorMapper for ColorMap<F>
where
    F: Interpolator,
{
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8> {
        self.compute_pixel(query)
    }
}

#[derive(Default)]
pub struct StepInterpolator {
    pub threshold: f32,
}

impl Interpolator for StepInterpolator {
    fn interpolate(
        &self,
        query: f32,
        value_zero: &Vector3<f32>,
        value_one: &Vector3<f32>,
    ) -> Vector3<f32> {
        if query > self.threshold {
            *value_one
        } else {
            *value_zero
        }
    }
}

#[derive(Default)]
pub struct LinearInterpolator {}

impl Interpolator for LinearInterpolator {
    fn interpolate(
        &self,
        query: f32,
        value_zero: &Vector3<f32>,
        value_one: &Vector3<f32>,
    ) -> Vector3<f32> {
        value_zero + (value_one - value_zero) * query
    }
}

/**
 * Create a new keyframe vector, using the same colors, but uniformly spaced queries.
 */
pub fn with_uniform_spacing(old_keys: &[ColorMapKeyFrame]) -> Vec<ColorMapKeyFrame> {
    let queries = lin_space(0.0..=1.0, old_keys.len());
    let mut new_keys = old_keys.to_vec();
    for (query, key) in queries.zip(&mut new_keys) {
        key.query = query;
    }
    new_keys
}

/**
 * Wrapper around a color map that precomputes a look-up table mapping from query
 * to the resulting color. This makes evaluation much faster.
 */
pub struct ColorMapLookUpTable {
    pub table: LookupTable<image::Rgb<u8>>,
}

impl Default for ColorMapLookUpTable {
    fn default() -> Self {
        Self {
            table: LookupTable::new([0.0, 1.0], 1, |_| Rgb([0, 0, 0])),
        }
    }
}

impl ColorMapLookUpTable {
    pub fn new<F: ColorMapper>(color_map: &F, entry_count: usize) -> ColorMapLookUpTable {
        ColorMapLookUpTable {
            table: LookupTable::new([0.0, 1.0], entry_count, |query: f32| {
                color_map.compute_pixel(query)
            }),
        }
    }

    // pub fn reset<F>(&mut self, query_domain: [f32; 2], query_to_data: F)
    // where
    //     F: Fn(f32) -> image::Rgb<u8>,
    // {
    //     self.table.reset(query_domain, query_to_data);
    // }
}

impl ColorMapper for ColorMapLookUpTable {
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8> {
        self.table.lookup(query)
    }
}
