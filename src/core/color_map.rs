use egui::Color32;
use image::Rgb;
use iter_num_tools::lin_space;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

use crate::core::interpolation::{
    InterpolationKeyframe, Interpolator, KeyframeInterpolator, LinearInterpolator,
};
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

/// Solid foreground / solid background pair. Used by basin-style fractals
/// (e.g. driven-damped pendulum) where a per-pixel `Option<i32>` selects one
/// or the other.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ForegroundBackground {
    /// Color used for the "in-set" / zeroth-basin pixel value.
    pub foreground: [u8; 3],
    /// Color used for every other pixel value (including non-converged).
    pub background: [u8; 3],
}

/// One gradient plus a solid background. Used by escape-time fractals
/// (Mandelbrot, Julia) where escaped pixels go through the gradient and
/// non-escaped pixels take the background color.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackgroundWithColorMap {
    /// Color used for pixels that did not produce a smooth-iteration value.
    pub background: [u8; 3],
    /// Keyframes describing the gradient applied to escaped pixels.
    pub color_map: Vec<ColorMapKeyFrame>,
}

/// Multiple gradients plus a solid "didn't converge" color. Used by Newton's
/// method, where a per-pixel `(smooth_iter, root_index)` picks the gradient.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultiColorMap {
    /// Color used when Newton iteration enters a cyclic attractor and never
    /// converges to a root.
    pub cyclic_attractor: [u8; 3],
    /// One gradient per root. The root index selects which gradient applies
    /// to a given pixel.
    pub color_maps: Vec<Vec<ColorMapKeyFrame>>,
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
 *   The `ColorMap` struct is implemented as a KeyframeInterpolator
 */
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

/// Pairs a color-map shape with its per-(sub)pixel cell type and a cached
/// form suitable for the colorize hot path. The `Cache` is allocated once at
/// pipeline construction and rebuilt in place by `refresh_cache`.
///
/// Phase 2 invariant: cells handed to `colorize_cell` for the scalar variants
/// have already been through `Renderable::normalize_field` (so the inner f32
/// is a CDF percentile in [0, 1]).
// `ColorMapKind` is consumed by `RenderingPipeline` (Phase 2.1, parallel to
// the legacy path). Its methods appear unused inside the lib until Phase 2.2
// wires the pipeline into the production runtime.
#[allow(dead_code)]
pub trait ColorMapKind: Sized + Sync {
    /// Per-(sub)pixel value the color map consumes.
    type Cell: Copy + Send + Sync;

    /// Allocation-once cache holding lookup tables and pre-converted
    /// `Color32` flat colors. Mutated in place by `refresh_cache`.
    type Cache: Send + Sync;

    /// One-time allocation at pipeline construction. `lookup_table_count`
    /// applies to variants that hold one or more `ColorMapLookUpTable`s.
    fn create_cache(&self, lookup_table_count: usize) -> Self::Cache;

    /// In-place rebuild of the cache from current keyframes / flat colors.
    /// Allocation-free.
    fn refresh_cache(&self, cache: &mut Self::Cache);

    /// Per-cell color lookup. Statically dispatched; called inside the
    /// AA-collapse loop. No allocation, no `dyn`.
    fn colorize_cell(cache: &Self::Cache, cell: Self::Cell) -> [u8; 3];
}

#[allow(dead_code)] // Phase 2.2 wires this in.
fn color32_to_rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

impl ColorMapKind for ForegroundBackground {
    type Cell = Option<i32>;
    /// `(foreground, background)` pre-converted to `Color32`.
    type Cache = (Color32, Color32);

    fn create_cache(&self, _lookup_table_count: usize) -> Self::Cache {
        (
            Color32::from_rgb(self.foreground[0], self.foreground[1], self.foreground[2]),
            Color32::from_rgb(self.background[0], self.background[1], self.background[2]),
        )
    }

    fn refresh_cache(&self, cache: &mut Self::Cache) {
        cache.0 = Color32::from_rgb(self.foreground[0], self.foreground[1], self.foreground[2]);
        cache.1 = Color32::from_rgb(self.background[0], self.background[1], self.background[2]);
    }

    fn colorize_cell(cache: &Self::Cache, cell: Self::Cell) -> [u8; 3] {
        if cell == Some(0) {
            color32_to_rgb(cache.0)
        } else {
            color32_to_rgb(cache.1)
        }
    }
}

impl ColorMapKind for BackgroundWithColorMap {
    type Cell = Option<f32>;
    /// `(gradient_lookup_table, background_color)` — table indexed over
    /// `[0, 1]`, populated from the keyframes.
    type Cache = (ColorMapLookUpTable, Color32);

    fn create_cache(&self, lookup_table_count: usize) -> Self::Cache {
        let inner = ColorMap::new(&self.color_map, LinearInterpolator);
        let table = ColorMapLookUpTable::new(lookup_table_count, [0.0, 1.0], &|q: f32| {
            inner.compute_pixel(q)
        });
        let bg = Color32::from_rgb(self.background[0], self.background[1], self.background[2]);
        (table, bg)
    }

    fn refresh_cache(&self, cache: &mut Self::Cache) {
        let inner = ColorMap::new(&self.color_map, LinearInterpolator);
        cache.0.reset([0.0, 1.0], &|q: f32| inner.compute_pixel(q));
        cache.1 = Color32::from_rgb(self.background[0], self.background[1], self.background[2]);
    }

    fn colorize_cell(cache: &Self::Cache, cell: Self::Cell) -> [u8; 3] {
        match cell {
            Some(p) => {
                let rgb: Rgb<u8> = cache.0.compute_pixel(p);
                [rgb[0], rgb[1], rgb[2]]
            }
            None => color32_to_rgb(cache.1),
        }
    }
}

impl ColorMapKind for MultiColorMap {
    type Cell = Option<(f32, u32)>;
    /// Per-root lookup tables (allocated once, sized to `color_maps.len()`)
    /// plus the cyclic-attractor flat color.
    type Cache = (Vec<ColorMapLookUpTable>, Color32);

    fn create_cache(&self, lookup_table_count: usize) -> Self::Cache {
        assert!(
            !self.color_maps.is_empty(),
            "MultiColorMap must define at least one gradient"
        );
        let tables = self
            .color_maps
            .iter()
            .map(|kfs| {
                let inner = ColorMap::new(kfs, LinearInterpolator);
                ColorMapLookUpTable::new(lookup_table_count, [0.0, 1.0], &|q: f32| {
                    inner.compute_pixel(q)
                })
            })
            .collect();
        let cyclic = Color32::from_rgb(
            self.cyclic_attractor[0],
            self.cyclic_attractor[1],
            self.cyclic_attractor[2],
        );
        (tables, cyclic)
    }

    fn refresh_cache(&self, cache: &mut Self::Cache) {
        debug_assert_eq!(
            cache.0.len(),
            self.color_maps.len(),
            "MultiColorMap cache size must match color_maps length; \
             root count is fixed for the session"
        );
        for (table, kfs) in cache.0.iter_mut().zip(self.color_maps.iter()) {
            let inner = ColorMap::new(kfs, LinearInterpolator);
            table.reset([0.0, 1.0], &|q: f32| inner.compute_pixel(q));
        }
        cache.1 = Color32::from_rgb(
            self.cyclic_attractor[0],
            self.cyclic_attractor[1],
            self.cyclic_attractor[2],
        );
    }

    fn colorize_cell(cache: &Self::Cache, cell: Self::Cell) -> [u8; 3] {
        match cell {
            Some((p, k)) => {
                let idx = (k as usize) % cache.0.len();
                let rgb: Rgb<u8> = cache.0[idx].compute_pixel(p);
                [rgb[0], rgb[1], rgb[2]]
            }
            None => color32_to_rgb(cache.1),
        }
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

    #[test]
    fn test_foreground_background_serde_roundtrip() {
        let original = ForegroundBackground {
            foreground: [255, 128, 0],
            background: [10, 20, 30],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ForegroundBackground = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.foreground, original.foreground);
        assert_eq!(parsed.background, original.background);
    }

    #[test]
    fn test_background_with_color_map_serde_roundtrip() {
        let original = BackgroundWithColorMap {
            background: [0, 0, 0],
            color_map: vec![
                ColorMapKeyFrame {
                    query: 0.0,
                    rgb_raw: [10, 20, 30],
                },
                ColorMapKeyFrame {
                    query: 1.0,
                    rgb_raw: [200, 210, 220],
                },
            ],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: BackgroundWithColorMap = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.background, original.background);
        assert_eq!(parsed.color_map.len(), original.color_map.len());
        for (a, b) in parsed.color_map.iter().zip(original.color_map.iter()) {
            assert_eq!(a.query, b.query);
            assert_eq!(a.rgb_raw, b.rgb_raw);
        }
    }

    #[test]
    fn test_multi_color_map_serde_roundtrip() {
        let original = MultiColorMap {
            cyclic_attractor: [255, 255, 255],
            color_maps: vec![
                vec![
                    ColorMapKeyFrame {
                        query: 0.0,
                        rgb_raw: [1, 2, 3],
                    },
                    ColorMapKeyFrame {
                        query: 1.0,
                        rgb_raw: [4, 5, 6],
                    },
                ],
                vec![ColorMapKeyFrame {
                    query: 0.5,
                    rgb_raw: [7, 8, 9],
                }],
            ],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: MultiColorMap = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cyclic_attractor, original.cyclic_attractor);
        assert_eq!(parsed.color_maps.len(), original.color_maps.len());
        for (a, b) in parsed.color_maps.iter().zip(original.color_maps.iter()) {
            assert_eq!(a.len(), b.len());
            for (ka, kb) in a.iter().zip(b.iter()) {
                assert_eq!(ka.query, kb.query);
                assert_eq!(ka.rgb_raw, kb.rgb_raw);
            }
        }
    }

    /// Helper: two-stop gradient from `red` to `blue` (positions 0 and 1).
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
    fn foreground_background_colorize_cell_dispatch() {
        let map = ForegroundBackground {
            foreground: [10, 20, 30],
            background: [200, 210, 220],
        };
        let cache = map.create_cache(0);

        // Some(0) -> foreground, anything else (including None) -> background.
        assert_eq!(
            ForegroundBackground::colorize_cell(&cache, Some(0)),
            [10, 20, 30]
        );
        assert_eq!(
            ForegroundBackground::colorize_cell(&cache, Some(7)),
            [200, 210, 220]
        );
        assert_eq!(
            ForegroundBackground::colorize_cell(&cache, None),
            [200, 210, 220]
        );
    }

    #[test]
    fn foreground_background_refresh_cache_picks_up_edits() {
        let mut map = ForegroundBackground {
            foreground: [1, 2, 3],
            background: [4, 5, 6],
        };
        let mut cache = map.create_cache(0);
        map.foreground = [99, 100, 101];
        map.refresh_cache(&mut cache);
        assert_eq!(
            ForegroundBackground::colorize_cell(&cache, Some(0)),
            [99, 100, 101]
        );
    }

    #[test]
    fn background_with_color_map_colorize_cell_endpoints_and_background() {
        let map = BackgroundWithColorMap {
            background: [9, 9, 9],
            color_map: red_to_blue(),
        };
        let cache = map.create_cache(256);
        // None -> background.
        assert_eq!(
            BackgroundWithColorMap::colorize_cell(&cache, None),
            [9, 9, 9]
        );
        // Endpoints land on the gradient.
        assert_eq!(
            BackgroundWithColorMap::colorize_cell(&cache, Some(0.0)),
            [255, 0, 0]
        );
        assert_eq!(
            BackgroundWithColorMap::colorize_cell(&cache, Some(1.0)),
            [0, 0, 255]
        );
        // Mid-gradient lookup is monotone between the endpoints.
        let mid = BackgroundWithColorMap::colorize_cell(&cache, Some(0.5));
        assert!(mid[0] > 0 && mid[0] < 255);
        assert_eq!(mid[1], 0);
        assert!(mid[2] > 0 && mid[2] < 255);
    }

    #[test]
    fn background_with_color_map_refresh_cache_picks_up_keyframe_edits() {
        let mut map = BackgroundWithColorMap {
            background: [0, 0, 0],
            color_map: red_to_blue(),
        };
        let mut cache = map.create_cache(64);
        // Mutate the underlying gradient and refresh the cache.
        map.color_map[0].rgb_raw = [50, 60, 70];
        map.color_map[1].rgb_raw = [80, 90, 100];
        map.refresh_cache(&mut cache);
        assert_eq!(
            BackgroundWithColorMap::colorize_cell(&cache, Some(0.0)),
            [50, 60, 70]
        );
        assert_eq!(
            BackgroundWithColorMap::colorize_cell(&cache, Some(1.0)),
            [80, 90, 100]
        );
    }

    #[test]
    fn multi_color_map_colorize_cell_picks_correct_root_table() {
        let map = MultiColorMap {
            cyclic_attractor: [42, 42, 42],
            color_maps: vec![
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
        let cache = map.create_cache(256);
        // None -> cyclic attractor.
        assert_eq!(MultiColorMap::colorize_cell(&cache, None), [42, 42, 42]);
        // Root 0 endpoint.
        assert_eq!(
            MultiColorMap::colorize_cell(&cache, Some((0.0, 0))),
            [255, 0, 0]
        );
        // Root 1 endpoint.
        assert_eq!(
            MultiColorMap::colorize_cell(&cache, Some((0.0, 1))),
            [0, 200, 0]
        );
        // Out-of-range root index wraps via modulo.
        assert_eq!(
            MultiColorMap::colorize_cell(&cache, Some((0.0, 2))),
            [255, 0, 0]
        );
    }

    #[test]
    fn multi_color_map_refresh_cache_keeps_outer_vec_capacity() {
        let mut map = MultiColorMap {
            cyclic_attractor: [0, 0, 0],
            color_maps: vec![red_to_blue(), red_to_blue()],
        };
        let mut cache = map.create_cache(32);
        let cap_before = cache.0.capacity();
        // Edit colors, refresh repeatedly; outer Vec must not grow.
        map.color_maps[0][1].rgb_raw = [10, 20, 30];
        map.refresh_cache(&mut cache);
        map.refresh_cache(&mut cache);
        assert_eq!(cache.0.capacity(), cap_before);
        assert_eq!(cache.0.len(), 2);
    }
}
