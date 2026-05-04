use egui::Color32;
use image::Rgb;
use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

use crate::core::histogram::{CumulativeDistributionFunction, Histogram};
use crate::core::interpolation::{
    InterpolationKeyframe, Interpolator, KeyframeInterpolator, LinearInterpolator,
};
use crate::core::lookup_table::LookupTable;

/// Represents a single "keyframe" of a color gradient, pairing a
/// "query" with the color that should be produced at that query point.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ColorMapKeyFrame {
    /// Location of this color within the gradient; on `[0, 1]`.
    pub query: f32,
    /// `[R, G, B]` triple, in raw 8-bit channels.
    pub rgb_raw: [u8; 3],
}

/// Unified color-map shape used by every `Renderable` fractal.
///
/// Mandelbrot, Julia, and DDP carry `gradients.len() == 1`; Newton's
/// method carries one gradient per root. The `u32` produced by
/// `FieldKernel::evaluate` indexes into `gradients` to pick which
/// gradient (and therefore which CDF / LUT) the cell colorizes through.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorMap {
    /// Color used for cells whose `evaluate` returned `None` (Mandelbrot
    /// in-set, DDP out-of-basin, Newton non-converging).
    pub flat_color: [u8; 3],
    /// One gradient per "channel". Length must be ≥ 1; rejected at
    /// deserialization otherwise. Each gradient is a `Vec<ColorMapKeyFrame>`.
    #[serde(deserialize_with = "deserialize_non_empty_gradients")]
    pub gradients: Vec<Vec<ColorMapKeyFrame>>,
}

fn deserialize_non_empty_gradients<'de, D>(
    deserializer: D,
) -> Result<Vec<Vec<ColorMapKeyFrame>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let gradients: Vec<Vec<ColorMapKeyFrame>> = Vec::deserialize(deserializer)?;
    if gradients.is_empty() {
        return Err(serde::de::Error::custom(
            "ColorMap.gradients must contain at least one gradient",
        ));
    }
    Ok(gradients)
}

/// Allocation-once cache used by the colorize hot path. The pipeline
/// owns one of these and refreshes it in place each frame.
pub struct ColorMapCache {
    /// Per-gradient CDF, refreshed by the pipeline after each compute pass.
    /// Length matches `ColorMap::gradients.len()`.
    pub cdfs: Vec<CumulativeDistributionFunction>,
    /// Per-gradient `[0, 1]`-domain lookup table. Refreshed by
    /// `ColorMap::refresh_cache` from the current keyframes.
    pub luts: Vec<ColorMapLookUpTable>,
    /// `flat_color` pre-converted to `Color32`.
    pub flat: Color32,
}

impl ColorMap {
    /// Allocate the cache once at pipeline construction.
    ///
    /// The CDFs are allocated against the supplied `histogram_bin_count`
    /// and `histogram_max_value` so the pipeline can reuse them in place
    /// across frames; their contents are recomputed from each frame's
    /// per-gradient histogram.
    pub fn create_cache(
        &self,
        histogram_bin_count: usize,
        histogram_max_value: f32,
        lookup_table_count: usize,
    ) -> ColorMapCache {
        assert!(
            !self.gradients.is_empty(),
            "ColorMap.gradients must contain at least one gradient"
        );
        let cdfs = self
            .gradients
            .iter()
            .map(|_| {
                let hist = Histogram::new(histogram_bin_count, histogram_max_value);
                CumulativeDistributionFunction::new(&hist)
            })
            .collect();
        let luts = self
            .gradients
            .iter()
            .map(|kfs| {
                let inner = KeyframeColorMap::new(kfs, LinearInterpolator);
                ColorMapLookUpTable::new(lookup_table_count, [0.0, 1.0], &|q: f32| {
                    inner.compute_pixel(q)
                })
            })
            .collect();
        let flat = Color32::from_rgb(self.flat_color[0], self.flat_color[1], self.flat_color[2]);
        ColorMapCache { cdfs, luts, flat }
    }

    /// Refresh `flat` and the per-gradient `luts` from the current keyframes
    /// and `flat_color`. Allocation-free; does NOT touch `cdfs` (those are
    /// owned by the pipeline and refreshed from histograms after each
    /// compute pass).
    pub fn refresh_cache(&self, cache: &mut ColorMapCache) {
        debug_assert_eq!(
            cache.luts.len(),
            self.gradients.len(),
            "ColorMapCache LUT count must match ColorMap gradient count; \
             gradient count is fixed for the session"
        );
        for (lut, kfs) in cache.luts.iter_mut().zip(self.gradients.iter()) {
            let inner = KeyframeColorMap::new(kfs, LinearInterpolator);
            lut.reset([0.0, 1.0], &|q: f32| inner.compute_pixel(q));
        }
        cache.flat = Color32::from_rgb(self.flat_color[0], self.flat_color[1], self.flat_color[2]);
    }
}

/// Per-cell color lookup. Statically dispatched; called inside the
/// AA-collapse loop. CDF percentile lookup happens here, in color space —
/// the field stays raw end-to-end.
#[inline]
pub fn colorize_cell(cache: &ColorMapCache, cell: Option<(f32, u32)>) -> [u8; 3] {
    match cell {
        Some((value, gradient_index)) => {
            let n = cache.luts.len();
            let g = (gradient_index as usize) % n.max(1);
            let pct = cache.cdfs[g].percentile(value);
            let rgb: Rgb<u8> = cache.luts[g].compute_pixel(pct);
            [rgb[0], rgb[1], rgb[2]]
        }
        None => [cache.flat.r(), cache.flat.g(), cache.flat.b()],
    }
}

/// Trait implemented by anything that maps a query in `[0, 1]` to an RGB
/// color. Used by `color_swatch` and the editor preview.
pub trait ColorMapper {
    /// Produce the color at the given query.
    fn compute_pixel(&self, query: f32) -> image::Rgb<u8>;
}

/// Piecewise-linear color map driven by an interpolator over a list of
/// keyframes. Used internally to populate `ColorMapLookUpTable`s.
pub struct KeyframeColorMap<F>
where
    F: Interpolator<f32, Vector3<f32>>,
{
    interpolator: KeyframeInterpolator<f32, Vector3<f32>, F>,
}

impl<F> KeyframeColorMap<F>
where
    F: Interpolator<f32, Vector3<f32>>,
{
    /// Construct a keyframe-driven color map. Keyframes must be non-empty
    /// and span the unit interval (first query 0.0, last query 1.0).
    pub fn new(keyframes: &[ColorMapKeyFrame], interpolator: F) -> Self {
        assert!(!keyframes.is_empty(), "keyframes must not be empty");
        assert!(
            keyframes.first().unwrap().query == 0.0,
            "first keyframe query must be 0.0"
        );
        assert!(
            keyframes.last().unwrap().query == 1.0,
            "last keyframe query must be 1.0"
        );
        let internal_keyframes: Vec<InterpolationKeyframe<f32, Vector3<f32>>> = keyframes
            .iter()
            .map(|kf| InterpolationKeyframe {
                query: kf.query,
                value: Vector3::new(
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

impl<F> ColorMapper for KeyframeColorMap<F>
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

/// Create a new keyframe vector with the same colors but uniformly spaced
/// queries.
pub fn with_uniform_spacing(old_keys: &[ColorMapKeyFrame]) -> Vec<ColorMapKeyFrame> {
    let queries = lin_space(0.0..=1.0, old_keys.len());
    let mut new_keys = old_keys.to_vec();
    for (query, key) in queries.zip(&mut new_keys) {
        key.query = query;
    }
    new_keys
}

/// Wrapper around a color map that precomputes a lookup table mapping from
/// query to the resulting color. Makes evaluation much faster on the hot
/// path.
pub struct ColorMapLookUpTable {
    /// The underlying lookup table, indexed over `[0, 1]`.
    pub table: LookupTable<image::Rgb<u8>>,
}

impl ColorMapLookUpTable {
    /// Construct a lookup table from a `ColorMapper`.
    pub fn from_color_map<F: ColorMapper>(
        color_map: &F,
        entry_count: usize,
    ) -> ColorMapLookUpTable {
        ColorMapLookUpTable::new(entry_count, [0.0, 1.0], &|query: f32| {
            color_map.compute_pixel(query)
        })
    }

    /// Construct a lookup table from an arbitrary closure over the query domain.
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

    /// Refresh the table in place, without allocating.
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

    /// Maps between black and some pre-specified color.
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

    /// Two-stop gradient from red (`[255, 0, 0]`) at 0.0 to blue
    /// (`[0, 0, 255]`) at 1.0.
    fn red_to_blue() -> Vec<ColorMapKeyFrame> {
        vec![
            ColorMapKeyFrame {
                query: 0.0,
                rgb_raw: [255, 0, 0],
            },
            ColorMapKeyFrame {
                query: 1.0,
                rgb_raw: [0, 0, 255],
            },
        ]
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
            green: 0.0,
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

    #[test]
    fn color_map_serde_round_trip() {
        let original = ColorMap {
            flat_color: [10, 20, 30],
            gradients: vec![red_to_blue()],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ColorMap = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.flat_color, original.flat_color);
        assert_eq!(parsed.gradients.len(), original.gradients.len());
        assert_eq!(parsed.gradients[0].len(), original.gradients[0].len());
    }

    #[test]
    fn color_map_rejects_empty_gradients_at_deserialization() {
        let json = r#"{
            "flat_color": [0, 0, 0],
            "gradients": []
        }"#;
        let result: Result<ColorMap, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "an empty `gradients` list must fail to deserialize"
        );
    }

    #[test]
    fn colorize_cell_uses_flat_color_for_none() {
        let map = ColorMap {
            flat_color: [9, 9, 9],
            gradients: vec![red_to_blue()],
        };
        let cache = map.create_cache(8, 1.0, 256);
        assert_eq!(colorize_cell(&cache, None), [9, 9, 9]);
    }

    #[test]
    fn colorize_cell_routes_to_correct_gradient() {
        let map = ColorMap {
            flat_color: [42, 42, 42],
            gradients: vec![
                red_to_blue(),
                vec![
                    ColorMapKeyFrame {
                        query: 0.0,
                        rgb_raw: [0, 200, 0],
                    },
                    ColorMapKeyFrame {
                        query: 1.0,
                        rgb_raw: [200, 0, 0],
                    },
                ],
            ],
        };
        // Bin count 1, max value 1.0 → CDF maps any value to 1.0 (all data
        // in the single bin), which lands on the high keyframe.
        let mut cache = map.create_cache(4, 1.0, 256);
        // Populate fake CDFs so percentile resolves predictably: insert
        // the same value into both histograms and rebuild the CDFs.
        let h0 = Histogram::new(4, 1.0);
        h0.insert(0.5);
        cache.cdfs[0] = CumulativeDistributionFunction::new(&h0);
        let h1 = Histogram::new(4, 1.0);
        h1.insert(0.5);
        cache.cdfs[1] = CumulativeDistributionFunction::new(&h1);

        // Gradient 0 maps low percentiles toward red, high toward blue.
        let g0_low = colorize_cell(&cache, Some((0.0, 0)));
        let g0_high = colorize_cell(&cache, Some((1.0, 0)));
        assert_eq!(g0_low, [255, 0, 0]);
        assert_eq!(g0_high, [0, 0, 255]);

        // Gradient 1 maps low percentiles toward green, high toward red.
        let g1_low = colorize_cell(&cache, Some((0.0, 1)));
        let g1_high = colorize_cell(&cache, Some((1.0, 1)));
        assert_eq!(g1_low, [0, 200, 0]);
        assert_eq!(g1_high, [200, 0, 0]);
    }

    #[test]
    fn colorize_cell_wraps_out_of_range_gradient_index() {
        let map = ColorMap {
            flat_color: [0, 0, 0],
            gradients: vec![red_to_blue()],
        };
        let mut cache = map.create_cache(4, 1.0, 256);
        let h = Histogram::new(4, 1.0);
        h.insert(0.0);
        cache.cdfs[0] = CumulativeDistributionFunction::new(&h);

        // gradient index 2 wraps to 0 via modulo.
        let rgb = colorize_cell(&cache, Some((0.0, 2)));
        assert_eq!(rgb, [255, 0, 0]);
    }

    #[test]
    fn refresh_cache_picks_up_keyframe_edits() {
        let mut map = ColorMap {
            flat_color: [0, 0, 0],
            gradients: vec![red_to_blue()],
        };
        let mut cache = map.create_cache(4, 1.0, 64);
        // Mutate a keyframe and refresh.
        map.gradients[0][0].rgb_raw = [50, 60, 70];
        map.refresh_cache(&mut cache);
        // Force the CDF to map 0.0 → 0.0 so the lookup hits the low keyframe.
        let h = Histogram::new(4, 1.0);
        h.insert(0.5);
        cache.cdfs[0] = CumulativeDistributionFunction::new(&h);
        assert_eq!(colorize_cell(&cache, Some((0.0, 0))), [50, 60, 70]);
    }

    #[test]
    fn refresh_cache_picks_up_flat_color_edits() {
        let mut map = ColorMap {
            flat_color: [1, 2, 3],
            gradients: vec![red_to_blue()],
        };
        let mut cache = map.create_cache(4, 1.0, 64);
        map.flat_color = [99, 100, 101];
        map.refresh_cache(&mut cache);
        assert_eq!(colorize_cell(&cache, None), [99, 100, 101]);
    }
}
