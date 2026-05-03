//! Top-level orchestrator that owns all reusable buffers and drives the
//! four-phase render pipeline:
//!
//! 1. (a) `compute_raw_field` — fill the field with raw, un-normalized values.
//! 2. (b) `populate_histogram` — walk the populated cells and insert into the
//!    histogram (no-op for fractals that don't normalize).
//! 3. (c) `normalize_field` — replace each populated cell's raw value with
//!    its CDF percentile in place (no-op for fractals that don't normalize).
//! 4. (d) `color_map().refresh_cache` + `colorize_collapse` — rebuild the
//!    colorize cache and walk the output `egui::ColorImage`, averaging
//!    `(n+1)²` subpixel `[u8; 3]` results into each output pixel.
//!
//! All buffers are allocated once at construction (or `resize`); per-frame
//! and per-pixel allocations are zero. Dispatch is fully monomorphized over
//! `F: Renderable`; there is no `dyn` or runtime variant matching on the hot
//! path.
//!
//! ## Phase 2.1 scope
//!
//! Only `sampling_level >= 0` is supported. Negative (block-fill) sampling
//! lands in 2.2 alongside the unified `sampling_level` field on
//! `RenderOptions`.

use egui::{Color32, ColorImage};
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;

use crate::core::color_map::ColorMapKind;
use crate::core::histogram::{CumulativeDistributionFunction, Histogram};
use crate::core::image_utils::Renderable;

/// Top-level orchestrator that owns all reusable buffers for one fractal
/// instance and runs the four-phase pipeline against them on every render.
pub struct RenderingPipeline<F: Renderable> {
    /// The fractal whose `Renderable` impl drives compute / histogram /
    /// normalize phases.
    fractal: F,
    /// Subpixel field, sized at construction for `(n_max+1)·W × (n_max+1)·H`
    /// where `n_max+1` is derived from the user's JSON `sampling_level` /
    /// `subpixel_antialiasing`.
    field: Vec<Vec<<F::ColorMap as ColorMapKind>::Cell>>,
    /// Histogram allocated from the fractal's parameters (e.g. `histogram_bin_count`
    /// from `ColorMapParams` for quadratic-map fractals).
    histogram: Histogram,
    /// CDF rebuilt in place from `histogram` after each compute pass.
    cdf: CumulativeDistributionFunction,
    /// Allocation-once color cache (lookup tables and pre-converted flat
    /// `Color32`s). Refreshed in place each render.
    color_cache: <F::ColorMap as ColorMapKind>::Cache,
    /// Permanent upsample factor for the field. The runtime sampling level
    /// passed to `render` must equal `n_max_plus_1 - 1` for now (Phase 2.1);
    /// 2.2 introduces variable runtime sampling against the same field.
    n_max_plus_1: usize,
}

impl<F: Renderable> RenderingPipeline<F> {
    /// Construct a pipeline. Allocates all buffers based on the fractal's
    /// current image specification, render options, and color-map params.
    pub fn new(
        fractal: F,
        n_max_plus_1: usize,
        histogram_bin_count: usize,
        histogram_max_value: f32,
        lookup_table_count: usize,
    ) -> Self {
        assert!(n_max_plus_1 >= 1, "n_max_plus_1 must be at least 1");
        let spec = fractal.image_specification();
        let outer = (spec.resolution[0] as usize) * n_max_plus_1;
        let inner = (spec.resolution[1] as usize) * n_max_plus_1;
        let field = allocate_field::<F>(outer, inner);
        let histogram = Histogram::new(histogram_bin_count, histogram_max_value);
        let cdf = CumulativeDistributionFunction::new(&histogram);
        let color_cache = fractal.color_map().create_cache(lookup_table_count);
        Self {
            fractal,
            field,
            histogram,
            cdf,
            color_cache,
            n_max_plus_1,
        }
    }

    /// Run the full pipeline, writing one output pixel per cell of `out`.
    /// `sampling_level` is the runtime value driven by the adaptive
    /// regulator; positive values use the AA subpixel grid, negative
    /// values trigger block-fill, and `0` is baseline.
    pub fn render(&mut self, out: &mut ColorImage, sampling_level: i32) {
        debug_assert!(
            sampling_level < (self.n_max_plus_1 as i32),
            "runtime sampling_level cannot exceed the cap baked into the field buffer"
        );
        let spec = *self.fractal.image_specification();
        debug_assert_eq!(
            self.field.len(),
            (spec.resolution[0] as usize) * self.n_max_plus_1,
            "field outer dim must match (n_max+1)·W"
        );
        debug_assert_eq!(
            out.size,
            [spec.resolution[0] as usize, spec.resolution[1] as usize]
        );

        self.fractal
            .compute_raw_field(sampling_level, &mut self.field);
        self.histogram.reset();
        self.fractal
            .populate_histogram(sampling_level, &self.field, &self.histogram);
        self.cdf.reset(&self.histogram);
        self.fractal
            .normalize_field(sampling_level, &self.cdf, &mut self.field);
        self.fractal
            .color_map()
            .refresh_cache(&mut self.color_cache);
        colorize_collapse::<F::ColorMap>(
            &self.color_cache,
            &self.field,
            self.n_max_plus_1,
            sampling_level,
            out,
        );
    }

    /// Reference to the underlying fractal — used to read params for
    /// snapshot / diagnostics.
    pub fn fractal(&self) -> &F {
        &self.fractal
    }

    /// Mutable access to the fractal — used by `PixelGrid` / explore loops
    /// to push view changes and speed-optimization edits.
    pub fn fractal_mut(&mut self) -> &mut F {
        &mut self.fractal
    }
}

fn allocate_field<F: Renderable>(
    outer: usize,
    inner: usize,
) -> Vec<Vec<<F::ColorMap as ColorMapKind>::Cell>> {
    (0..outer)
        .map(|_| vec![<F::ColorMap as ColorMapKind>::Cell::default(); inner])
        .collect()
}

/// Walk the row-major output `egui::ColorImage`, collapsing field cells
/// into output pixels.
///
/// - **Positive `sampling_level = r`**: each output pixel `(px, py)` averages
///   the `(r+1)²` cells at `field[px·n_max_plus_1 + i][py·n_max_plus_1 + j]`
///   for `i, j ∈ 0..(r+1)`.
/// - **`sampling_level == 0`**: one cell per output pixel (the top-left of
///   each block).
/// - **Negative `sampling_level = -m`**: block-fill. Every `(m+1) × (m+1)`
///   output-pixel block reads the top-left field cell of the leftmost
///   output pixel of the block; all output pixels in the block share one
///   color. (Nearest-neighbor / zero-order hold; this replaces the legacy
///   `KeyframeLinearPixelInerpolation` interpolation.)
///
/// Generic over `C: ColorMapKind`; fully monomorphized at the call site.
/// Per-pixel allocations: zero.
pub fn colorize_collapse<C: ColorMapKind>(
    cache: &C::Cache,
    field: &[Vec<C::Cell>],
    n_max_plus_1: usize,
    sampling_level: i32,
    out: &mut ColorImage,
) {
    let width = out.size[0];

    if sampling_level >= 0 {
        let n = sampling_level as usize + 1;
        let count = (n * n) as u32;
        out.pixels
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(py, row)| {
                for (px, pixel) in row.iter_mut().enumerate() {
                    let mut sum = [0u32; 3];
                    for i in 0..n {
                        let cx = px * n_max_plus_1 + i;
                        let col = &field[cx];
                        for j in 0..n {
                            let cy = py * n_max_plus_1 + j;
                            let rgb = C::colorize_cell(cache, col[cy]);
                            sum[0] += rgb[0] as u32;
                            sum[1] += rgb[1] as u32;
                            sum[2] += rgb[2] as u32;
                        }
                    }
                    *pixel = Color32::from_rgb(
                        (sum[0] / count) as u8,
                        (sum[1] / count) as u8,
                        (sum[2] / count) as u8,
                    );
                }
            });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        out.pixels
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(py, row)| {
                let block_y = py / block_size;
                let cy = block_y * block_size * n_max_plus_1;
                for (px, pixel) in row.iter_mut().enumerate() {
                    let block_x = px / block_size;
                    let cx = block_x * block_size * n_max_plus_1;
                    let rgb = C::colorize_cell(cache, field[cx][cy]);
                    *pixel = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color_map::{ColorMapKeyFrame, ForegroundBackground};

    /// Synthetic 4×4 field of `Option<i32>` with a 2×2 average per output
    /// pixel (sampling_level = 1, so `n = 2`). Every subpixel in the
    /// top-left block is `Some(0)` (foreground), so the output pixel is
    /// pure foreground. Every subpixel in the top-right is `None`, so the
    /// output is pure background. Bottom row mixes 2 foreground + 2
    /// background → averaged channel is the integer mean.
    #[test]
    fn colorize_collapse_aa_averaging_matches_hand_computed() {
        let cm = ForegroundBackground {
            foreground: [200, 100, 0],
            background: [0, 50, 250],
        };
        let cache = cm.create_cache(0);

        // Output is 2×2; field is 4×4. field[x][y].
        let mut field: Vec<Vec<Option<i32>>> = vec![vec![None; 4]; 4];
        // Top-left output pixel (0, 0) ← block field[0..2][0..2]: all Some(0).
        field[0][0] = Some(0);
        field[0][1] = Some(0);
        field[1][0] = Some(0);
        field[1][1] = Some(0);
        // Top-right (1, 0) ← field[2..4][0..2]: all None (already initialized).
        // Bottom-left (0, 1) ← field[0..2][2..4]: 2 Some(0) + 2 None.
        field[0][2] = Some(0);
        field[0][3] = None;
        field[1][2] = Some(0);
        field[1][3] = None;
        // Bottom-right (1, 1) ← field[2..4][2..4]: all Some(7) (non-zero → bg).
        field[2][2] = Some(7);
        field[2][3] = Some(7);
        field[3][2] = Some(7);
        field[3][3] = Some(7);

        let mut out = ColorImage::filled([2, 2], Color32::BLACK);
        colorize_collapse::<ForegroundBackground>(&cache, &field, 2, 1, &mut out);

        // Output is row-major: out.pixels[py * width + px].
        let pixel_at = |px: usize, py: usize| out.pixels[py * 2 + px];
        // (0, 0): pure foreground.
        assert_eq!(pixel_at(0, 0), Color32::from_rgb(200, 100, 0));
        // (1, 0): pure background.
        assert_eq!(pixel_at(1, 0), Color32::from_rgb(0, 50, 250));
        // (0, 1): mean of (200,100,0) and (0,50,250) summed twice each:
        //   sum/4 per channel = (400/4, 300/4, 500/4) = (100, 75, 125).
        assert_eq!(pixel_at(0, 1), Color32::from_rgb(100, 75, 125));
        // (1, 1): all Some(7) → background (anything non-zero/None for FB).
        assert_eq!(pixel_at(1, 1), Color32::from_rgb(0, 50, 250));
    }

    /// `n = 1` (sampling_level = 0): one cell per output pixel, no averaging.
    #[test]
    fn colorize_collapse_no_aa_one_cell_per_pixel() {
        let cm = ForegroundBackground {
            foreground: [255, 0, 0],
            background: [0, 0, 255],
        };
        let cache = cm.create_cache(0);

        let mut field: Vec<Vec<Option<i32>>> = vec![vec![None; 2]; 2];
        field[0][0] = Some(0);
        field[1][1] = Some(0);

        let mut out = ColorImage::filled([2, 2], Color32::BLACK);
        colorize_collapse::<ForegroundBackground>(&cache, &field, 1, 0, &mut out);

        assert_eq!(out.pixels[0], Color32::from_rgb(255, 0, 0)); // (0,0)
        assert_eq!(out.pixels[1], Color32::from_rgb(0, 0, 255)); // (1,0)
        assert_eq!(out.pixels[2], Color32::from_rgb(0, 0, 255)); // (0,1)
        assert_eq!(out.pixels[3], Color32::from_rgb(255, 0, 0)); // (1,1)
    }

    /// Smoke test that the keyframe lookup is exercised by colorize_collapse
    /// for the gradient variant.
    #[test]
    fn colorize_collapse_background_with_color_map_endpoints() {
        use crate::core::color_map::BackgroundWithColorMap;
        let cm = BackgroundWithColorMap {
            background: [9, 9, 9],
            color_map: vec![
                ColorMapKeyFrame {
                    query: 0.0,
                    rgb_raw: [255, 0, 0],
                },
                ColorMapKeyFrame {
                    query: 1.0,
                    rgb_raw: [0, 0, 255],
                },
            ],
        };
        let cache = cm.create_cache(256);
        let mut field: Vec<Vec<Option<f32>>> = vec![vec![None; 1]; 1];
        field[0][0] = Some(0.0);
        let mut out = ColorImage::filled([1, 1], Color32::BLACK);
        colorize_collapse::<BackgroundWithColorMap>(&cache, &field, 1, 0, &mut out);
        assert_eq!(out.pixels[0], Color32::from_rgb(255, 0, 0));

        field[0][0] = Some(1.0);
        colorize_collapse::<BackgroundWithColorMap>(&cache, &field, 1, 0, &mut out);
        assert_eq!(out.pixels[0], Color32::from_rgb(0, 0, 255));

        field[0][0] = None;
        colorize_collapse::<BackgroundWithColorMap>(&cache, &field, 1, 0, &mut out);
        assert_eq!(out.pixels[0], Color32::from_rgb(9, 9, 9));
    }
}
