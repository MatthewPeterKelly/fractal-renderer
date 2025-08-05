use image::Rgb;
use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

use crate::core::interpolation::Interpolator;
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
            let val_low = self.queries[idx_low];
            let alpha = (query - val_low) / (self.queries[idx_upp] - val_low);
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
