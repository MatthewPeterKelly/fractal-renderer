use iter_num_tools::lin_space;
use serde::{Deserialize, Serialize};

/**
 * Represents a single "keyframe" of the color map, pairing a
 * "query" with the color that should be produced at that query point.
 */
#[derive(Serialize, Deserialize, Debug)]
pub struct ColorMapKeyFrame {
    pub query: f32,       // specify location of this color within the map; on [0,1]
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
    keyframes: Vec<ColorMapKeyFrame>,
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
        PiecewiseLinearColorMap { keyframes }
    }

    /**
     * Create a new color map with the same keyframe RGB values, but replace the
     * query values with a uniformly spaced set of queries. This is largely used
     * for visualizing the "color swatch".
     */
    pub fn with_uniform_spacing(&self) -> PiecewiseLinearColorMap {
        let queries = lin_space(0.0..=1.0, self.keyframes.len());
        let mut keyframes: Vec<ColorMapKeyFrame> = Vec::with_capacity(self.keyframes.len());
        for (old_keyframe, query) in self.keyframes.iter().zip(queries) {
            keyframes.push(ColorMapKeyFrame {
                query,
                rgb_raw: old_keyframe.rgb_raw,
            });
        }
        PiecewiseLinearColorMap::new(keyframes)
    }

    /**
     * Evaluates the color map, modestly efficient for small numbers of
     * keyframes. Any query outside of [0,1] will be clamped.
     */
    pub fn compute(&self, query: f32, clamp_to_nearest: bool) -> [u8; 3] {
        if query <= 0.0f32 {
            self.keyframes.first().unwrap().rgb_raw
        } else if query >= 1.0f32 {
            self.keyframes.last().unwrap().rgb_raw
        } else {
            let (i, j) = self.linear_index_search(query);
            let mut alpha = (query - self.keyframes[i].query)
                / (self.keyframes[j].query - self.keyframes[i].query);
            if clamp_to_nearest {
                alpha = PiecewiseLinearColorMap::clamp_alpha_nearest(alpha);
            }
            Self::interpolate(
                &self.keyframes[i].rgb_raw,
                &self.keyframes[j].rgb_raw,
                alpha,
            )
        }
    }

    /**
     * Simple linear search, starting from the middle segment, to figure out
     * which segment to evaluate. We could probably be faster by caching the most
     * recent index solution, but that adds complexity and state, which are probably
     * not worth it, given that the plan is to pre-compute the entire color map
     * before rendering the fractal.
     */
    fn linear_index_search(&self, query: f32) -> (usize, usize) {
        let mut idx_low = self.keyframes.len() / 2;

        // hard limit on upper iteration, to catch bugs
        for _ in 0..self.keyframes.len() {
            if query < self.keyframes[idx_low].query {
                idx_low -= 1;
                continue;
            }
            if query >= self.keyframes[idx_low + 1].query {
                idx_low += 1;
                continue;
            }
            // [low <= query < upp]  --> success!
            return (idx_low, idx_low + 1);
        }

        println!("ERROR:  Linear keyframe search failed!");
        panic!();
    }

    /**
     * Really simple color interpolation.
     * See the Palette crate for a lecture about a better way to do it:
     * https://docs.rs/palette/latest/palette/rgb/index.html
     *
     * I've got a version using that hacked together on a branch here:
     * https://github.com/MatthewPeterKelly/fractal-renderer/pull/71
     *
     * But this simple implementation works nicely for now.
     */
    fn interpolate(low: &[u8; 3], upp: &[u8; 3], alpha: f32) -> [u8; 3] {
        let beta = 1.0 - alpha;
        [
            ((low[0] as f32) * beta + (upp[0] as f32) * alpha) as u8,
            ((low[1] as f32) * beta + (upp[1] as f32) * alpha) as u8,
            ((low[2] as f32) * beta + (upp[2] as f32) * alpha) as u8,
        ]
    }

    /**
     * This is a bit of a hack, but it makes it easy to implement the
     * color swatch utility. And, who knows, perhaps it would be useful
     * to render a fractal with sharp color bands someday.
     */
    fn clamp_alpha_nearest(alpha: f32) -> f32 {
        if alpha < 0.5 {
            0.0
        } else {
            1.0
        }
    }
}
