use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

/**
 * Represents a single "keyframe" of the color map, pairing a
 * "query" with the color that should be produced at that query point.
 */
#[derive(Serialize, Deserialize, Debug)]
pub struct ColorMapKeyFrame {
    pub query: f32,        // specify location of this color within the map; on [0,1]
    pub rgb_raw: [u8; 3], // [R, G, B]
}

/**
 * Simple implementation of a "piecewise linear" color map, where the colors
 * are represented by simple linear interpolation in RGB color space. This is
 * not "strictly correct" from a color standpoint, but it works well enough in
 * practice. For details see:
 * - https://github.com/MatthewPeterKelly/fractal-renderer/pull/71
 * - https://docs.rs/palette/latest/palette/
 */
pub struct PiecewiseLinearColorMap {
    queries: Vec<f32>,
    rgb_colors: Vec<Vector3<f32>>,  // [0,255], but as f32
}

impl PiecewiseLinearColorMap {
    /**
     * Create a color map from a vector of keyframes. The queries must be
     * monotonically increasing, and the first keyframe query must be zero
     * and the last keyframe query must be one. Colors are specified in RGB
     * space as `u8` values on [0,255].
     */
    pub fn new(keyframes: Vec<ColorMapKeyFrame>) -> PiecewiseLinearColorMap {
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
            rgb_colors.push(Vector3::new(keyframe.rgb_raw[0] as f32, keyframe.rgb_raw[1] as f32, keyframe.rgb_raw[2] as f32));
        }

        PiecewiseLinearColorMap {
            queries,
            rgb_colors,
        }
    }

    /**
     * Create a new color map with the same keyframe RGB values, but replace the
     * query values with a uniformly spaced set of queries. This is largely used
     * for visualizing the "color swatch".
     */
    pub fn with_uniform_spacing(&self) -> PiecewiseLinearColorMap {
        PiecewiseLinearColorMap {
            queries: lin_space(0.0..=1.0, self.queries.len()).collect(),
            rgb_colors: self.rgb_colors.clone(),
        }
    }

    /**
     * Evaluates the color map, modestly efficient for small numbers of
     * keyframes. Any query outside of [0,1] will be clamped.
     */
    fn compute_raw(&self, query: f32, clamp_to_nearest: bool) -> Vector3<f32> {
        if query <= 0.0f32 {
            *self.rgb_colors.first().unwrap()
        } else if query >= 1.0f32 {
            *self.rgb_colors.last().unwrap()
        } else {
            let idx_low = linear_index_search(&self.queries, query);
            let idx_upp = idx_low + 1;

            if clamp_to_nearest {
                self.interpolate_nearest(query, idx_low,idx_upp)
            } else {
                self.interpolate_linear(query, idx_low,idx_upp)
            }
        }
    }

    pub fn compute_pixel(&self, query: f32, clamp_to_nearest: bool) -> image::Rgb<u8> {
        let color_rgb = self.compute_raw(query, clamp_to_nearest);
        image::Rgb([color_rgb[0] as u8, color_rgb[1] as u8, color_rgb[2] as u8])
    }

    fn interpolate_nearest(&self, query: f32, idx_low: usize, idx_upp: usize)-> Vector3<f32> {
        let low_delta = query - self.queries[idx_low];
        let upp_delta = self.queries[idx_upp] - query;
        if upp_delta > low_delta {
            self.rgb_colors[idx_low]
        } else {
            self.rgb_colors[idx_upp]
        }
    }

    fn interpolate_linear(&self, query: f32, idx_low: usize, idx_upp: usize)-> Vector3<f32> {
        let alpha = (query - self.queries[idx_low])
        / (self.queries[idx_upp] - self.queries[idx_low]);
        (1.0 - alpha) * self.rgb_colors[idx_low] +  alpha* self.rgb_colors[idx_upp]
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
