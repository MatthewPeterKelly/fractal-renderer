use image::Rgb;
use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

use crate::core::interpolation::{InterpolationKeyframe, Interpolator, KeyframeInterpolator};
use crate::core::lookup_table::LookupTable;

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
/// ColorMap is just a specific KeyframeInterpolator for f32 -> Vector3<f32>
pub struct ColorMap<F>
where
    F: Interpolator<f32, Vector3<f32>>,
{
    interpolator: KeyframeInterpolator<f32, Vector3<f32>, F>,
}

impl<F> ColorMap<F>
where
    F: Interpolator<f32, Vector3<f32>>,
{
    pub fn new(keyframes: &[ColorMapKeyFrame], interpolator: F) -> Self {
        let internal_keyframes: Vec<InterpolationKeyframe<f32, Vector3<f32>>> = keyframes
            .iter()
            .map(|kf| InterpolationKeyframe {
                input: kf.query,
                output: Vector3::new(
                    kf.rgb_raw[0] as f32,
                    kf.rgb_raw[1] as f32,
                    kf.rgb_raw[2] as f32,
                ),
            })
            .collect();

        let interpolator = KeyframeInterpolator::new(internal_keyframes, interpolator);

        Self { interpolator }
    }
}

impl<F> ColorMapper for ColorMap<F>
where
    F: Interpolator<f32, Vector3<f32>>,
{
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8> {
        let color: Vector3<f32> = self.interpolator.evaluate(query);
        image::Rgb([
            color[0].clamp(0.0, 255.0) as u8,
            color[1].clamp(0.0, 255.0) as u8,
            color[2].clamp(0.0, 255.0) as u8,
        ])
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

impl ColorMapLookUpTable {
    pub fn from_color_map<F: ColorMapper>(
        color_map: &F,
        entry_count: usize,
    ) -> ColorMapLookUpTable {
        ColorMapLookUpTable::new(entry_count, [0.0, 1.0], &|query: f32| {
            color_map.compute_pixel(query)
        })
    }

    pub fn new<F>(entry_count: usize, query_domain: [f32; 2], color_map: &F) -> ColorMapLookUpTable
    where
        F: Fn(f32) -> image::Rgb<u8>,
    {
        let mut map = ColorMapLookUpTable {
            table: LookupTable::new([0.0, 1.0], entry_count, |_| Rgb([0, 0, 0])),
        };
        map.reset(query_domain, color_map);
        map
    }

    pub fn reset<F>(&mut self, query_domain: [f32; 2], color_map: &F)
    where
        F: Fn(f32) -> image::Rgb<u8>,
    {
        self.table.reset(query_domain, color_map);
    }
}

impl ColorMapper for ColorMapLookUpTable {
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8> {
        self.table.lookup(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// Maps between black and some pre-specified color
    struct SimpleColorMap {
        red: f32,
        green: f32,
        blue: f32,
    }

    impl ColorMapper for SimpleColorMap {
        fn compute_pixel(&self, query: f32) -> Rgb<u8> {
            let alpha = query.clamp(0.0, 1.0);
            Rgb([
                (alpha * self.red).round() as u8,
                (alpha * self.green).round() as u8,
                (alpha * self.blue).round() as u8,
            ])
        }
    }

    #[test]
    fn test_color_map_lookup_table() {
        let simple_color_map = SimpleColorMap {
            red: 255.0,
            green: 255.0,
            blue: 255.0,
        };

        let mut table = ColorMapLookUpTable::new(40, [0.0, 1.0], &|query: f32| {
            simple_color_map.compute_pixel(query)
        });

        // We only have 40 entries... so we don't actually hit the "perfect middle"
        let mapped_half = 131;

        assert_eq!(table.compute_pixel(0.0), Rgb([0, 0, 0]));
        assert_eq!(table.compute_pixel(1.0), Rgb([255, 255, 255]));
        assert_eq!(
            table.compute_pixel(0.5),
            Rgb([mapped_half, mapped_half, mapped_half])
        );

        assert_eq!(table.compute_pixel(-1.0), Rgb([0, 0, 0]));
        assert_eq!(table.compute_pixel(2.0), Rgb([255, 255, 255]));

        let simple_color_map = SimpleColorMap {
            red: 255.0,
            green: 0.0, // drop green from the output of the map
            blue: 255.0,
        };
        table.reset([0.0, 1.0], &|query: f32| {
            simple_color_map.compute_pixel(query)
        });

        assert_eq!(table.compute_pixel(0.0), Rgb([0, 0, 0]));
        assert_eq!(table.compute_pixel(1.0), Rgb([255, 0, 255]));
        assert_eq!(table.compute_pixel(0.5), Rgb([mapped_half, 0, mapped_half]));

        assert_eq!(table.compute_pixel(-1.0), Rgb([0, 0, 0]));
        assert_eq!(table.compute_pixel(2.0), Rgb([255, 0, 255]));
    }
}
