use iter_num_tools::lin_space;
use serde::{Deserialize, Serialize};
use splines::{Interpolation, Key, Spline};
use nalgebra::Vector3;

/**
 * Represents a single "keyframe" of the color map, pairing a
 * "query" with the color that should be produced at that query point.
 */
#[derive(Serialize, Deserialize, Debug)]
pub struct ColorMapKeyFrame {
    pub query: f32,        // specify location of this color within the map; on [0,1]
    pub rgb_raw: Vector3<f32>, // [R, G, B], defined on [0.0, 1.0]
}

/**
 * Simple implementation of a "piecewise linear" color map, where the colors
 * are represented by simple linear interpolation in RGB color space. This is
 * not "strictly correct" from a color standpoint, but it works well enough in
 * practice. For details see:
 * - https://github.com/MatthewPeterKelly/fractal-renderer/pull/71
 * - https://docs.rs/palette/latest/palette/
 */
#[derive(Clone)]
pub struct PiecewiseLinearColorMap {
    spline: Spline<f32, Vector3<f32>>,
}

impl PiecewiseLinearColorMap {
    /**
     * Create a color map from a vector of keyframes. The queries must be
     * monotonically increasing, and the first keyframe query must be zero
     * and the last keyframe query must be one. Colors are specified in RGB
     * space as `u8` values on [0,255].
     */
    pub fn new(
        keyframes: Vec<ColorMapKeyFrame>,
        interpolation: Interpolation<f32, Vector3<f32>>,
    ) -> PiecewiseLinearColorMap {
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

        PiecewiseLinearColorMap {
            spline: Spline::from_iter(
                keyframes
                    .into_iter()
                    .map(|key| Key::new(key.query, key.rgb_raw, interpolation)),
            ),
        }
    }

    /**
     * Create a new color map with the same keyframe RGB values, but replace the
     * query values with a uniformly spaced set of queries. This is largely used
     * for visualizing the "color swatch".
     */
    pub fn with_uniform_spacing(&self) -> PiecewiseLinearColorMap {
        let queries = lin_space(0.0..=1.0, self.spline.keys().len());
        let mut colormap = self.clone();
        for (index, query) in queries.enumerate() {
            colormap.spline.replace(index, |old_key| {
                Key::new(query, old_key.value, old_key.interpolation)
            });
        }
        colormap
    }

    /**
     * Evaluates the color map, modestly efficient for small numbers of
     * keyframes. Any query outside of [0,1] will be clamped.
     */
    pub fn sample(&self, query: f32) -> Vector3<f32> {
        let first = self.spline.keys().first().unwrap();
        if query <= first.t {
           return first.value;
        }

        let last = self.spline.keys().last().unwrap();
        if query >= last.t {
           return last.value;
        }

        self.spline.sample(query).unwrap()
    }

}



// Broken --> type piracy!  Can't define trait on an external type.
// I don't really want to make my own custom vector type wrapping nalgebra.
// This seems a bit silly.
// Someone must have implemented this for nalgebra somehow.

impl splines::Interpolate<f32> for Vector3<f32> {
    fn step(t: f32, threshold: f32, a: Self, b: Self) -> Self;

    fn lerp(t: f32, a: Self, b: Self) -> Self;

    fn cosine(t: f32, a: Self, b: Self) -> Self;

    fn cubic_hermite(t: f32, x: (f32, Self), a: (f32, Self), b: (f32, Self), y: (f32, Self)) -> Self;

    fn quadratic_bezier(t: f32, a: Self, u: Self, b: Self) -> Self;

    fn cubic_bezier(t: f32, a: Self, u: Self, v: Self, b: Self) -> Self;

    fn cubic_bezier_mirrored(t: f32, a: Self, u: Self, v: Self, b: Self) -> Self;
  }
