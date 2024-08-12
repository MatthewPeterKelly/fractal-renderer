use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

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

/**
 * Simple implementation of a "piecewise linear" color map, where the colors
 * are represented by simple linear interpolation in RGB color space. This is
 * not "strictly correct" from a color standpoint, but it works well enough in
 * practice. For details see:
 * - https://github.com/MatthewPeterKelly/fractal-renderer/pull/71
 * - https://docs.rs/palette/latest/palette/
 */
pub struct ColorMap<F>
where
    F: Fn(f32, &Vector3<f32>, &Vector3<f32>) -> Vector3<f32>,
{
    queries: Vec<f32>,
    rgb_colors: Vec<Vector3<f32>>, // [0,255], but as f32
    interpolator: F,
}

impl<F> ColorMap<F>
where
    F: Fn(f32, &Vector3<f32>, &Vector3<f32>) -> Vector3<f32>,
{
    /**
     * Create a color map from a vector of keyframes. The queries must be
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
            let idx_low = linear_index_search(&self.queries, query);
            let idx_upp = idx_low + 1;
            let alpha =
                (query - self.queries[idx_low]) / (self.queries[idx_upp] - self.queries[idx_low]);
            (self.interpolator)(alpha, &self.rgb_colors[idx_low], &self.rgb_colors[idx_upp])
        }
    }
}

impl<F> ColorMapper for ColorMap<F>
where
    F: Fn(f32, &Vector3<f32>, &Vector3<f32>) -> Vector3<f32>,
{
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8> {
        self.compute_pixel(query)
    }
}

/**
 * Create a new keyframe vector, using the same colors, but uniformly spaced queries.
 */
pub fn with_uniform_spacing(old_keys: &Vec<ColorMapKeyFrame>) -> Vec<ColorMapKeyFrame> {
    let queries = lin_space(0.0..=1.0, old_keys.len());
    let mut new_keys = old_keys.clone();
    for (query, key) in queries.zip(&mut new_keys) {
        key.query = query;
    }
    new_keys
}

pub fn nearest_interpolator() -> impl Fn(f32, &Vector3<f32>, &Vector3<f32>) -> Vector3<f32> {
    move |alpha: f32, v0: &Vector3<f32>, v1: &Vector3<f32>| -> Vector3<f32> {
        if alpha > 0.5 {
            *v1
        } else {
            *v0
        }
    }
}

pub fn linear_interpolator() -> impl Fn(f32, &Vector3<f32>, &Vector3<f32>) -> Vector3<f32> {
    move |alpha: f32, v0: &Vector3<f32>, v1: &Vector3<f32>| -> Vector3<f32> {
        v0 + (v1 - v0) * alpha
    }
}

/**
 * Simple linear search, starting from the middle segment, to figure out
 * which segment to evaluate. We could probably be faster by caching the most
 * recent index solution, but that adds complexity and state, which are probably
 * not worth it, given that the plan is to pre-compute the entire color map
 * before rendering the fractal.
 *
 * Preconditions:
 * - `keys` is a sorted vector that is monotonically increasing
 * - `keys` has at least two entries
 * - `query` is spanned by the values in `keys`
 *
 * (Preconditions are not checked because they are enforced by the PiecewisLinearColorMap class invariants.)
 *
 * @return: `idx_low` S.T. keys[idx_low] < query < keys[idx_upp]
 */
fn linear_index_search(keys: &[f32], query: f32) -> usize {
    let mut idx_low = keys.len() / 2;

    // hard limit on upper iteration, to catch bugs
    for _ in 0..keys.len() {
        if query < keys[idx_low] {
            idx_low -= 1;
            continue;
        }
        if query >= keys[idx_low + 1] {
            idx_low += 1;
            continue;
        }
        // [low <= query < upp]  --> success!
        return idx_low;
    }

    println!("ERROR:  Linear keyframe search failed!");
    panic!();
}
