//! Top-level orchestrator that owns all reusable buffers and drives the
//! four-phase render pipeline:
//!
//! 1. (a) `field_iteration::compute_raw_field` — fill the field with raw
//!    `Option<(f32, u32)>` cells via the fractal's `FieldKernel::evaluate`.
//! 2. (b) `field_iteration::populate_histograms` — bin populated cells into
//!    the per-color-map histograms (owned by the cache).
//! 3. (c) `ColorPaletteCache::refresh_after_compute_pass` — atomically
//!    rebuild every downstream-visible piece of cache state (per-color-map
//!    CDFs from the freshly-populated histograms, per-color-map LUTs from
//!    the palette's keyframes, and the cached background color). Pulling
//!    all three into one call prevents the cache from drifting into a
//!    half-updated state between renders.
//! 4. (d) `field_iteration::colorize_collapse_unified` — walk the output
//!    `egui::ColorImage`, averaging `(n+1)²` subpixel `[u8; 3]` results
//!    into each output pixel via `colorize_cell`.
//!
//! All buffers are allocated once at construction (or `resize`); per-frame
//! and per-pixel allocations are zero. Dispatch is fully monomorphized over
//! `F: Renderable`; there is no `dyn` or runtime variant matching on the hot
//! path. The field stays raw end-to-end — there is no `normalize_field`
//! step; CDF lookup happens inside `colorize_cell` at colorize time.

use egui::ColorImage;

use crate::core::color_map::ColorPaletteCache;
use crate::core::field_iteration::{
    colorize_collapse_unified, compute_raw_field, populate_histograms,
};
use crate::core::image_utils::Renderable;

/// Top-level orchestrator that owns all reusable buffers for one fractal
/// instance and runs the four-phase pipeline against them on every render.
pub struct RenderingPipeline<F: Renderable> {
    /// The fractal whose `FieldKernel::evaluate` drives the compute phase.
    fractal: F,
    /// Subpixel field, sized at construction for `(n_max+1)·W × (n_max+1)·H`
    /// where `n_max+1` is derived from the user's JSON `sampling_level`.
    field: Vec<Vec<Option<(f32, u32)>>>,
    /// Allocation-once color cache (per-color-map histograms, CDFs, LUTs,
    /// and the pre-converted background `Color32`). The pipeline fills the
    /// histograms during (b), then `refresh_after_compute_pass` rebuilds
    /// the CDFs / LUTs / background atomically as one step.
    color_cache: ColorPaletteCache,
    /// Permanent upsample factor for the field. The runtime sampling level
    /// passed to `render` is at most `n_max_plus_1 - 1`.
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
        // The field buffer is sized as (n_max_plus_1·W) × (n_max_plus_1·H)
        // cells — one cell per sub-pixel slot at the maximum AA factor
        // baked in at construction. Runtime sampling levels at or below
        // that cap populate a sub-rectangle of this buffer; the remainder
        // stays untouched between frames.
        let outer_dim_x = (spec.resolution[0] as usize) * n_max_plus_1;
        let inner_dim_y = (spec.resolution[1] as usize) * n_max_plus_1;
        let field = (0..outer_dim_x).map(|_| vec![None; inner_dim_y]).collect();
        let color_cache = fractal.color_palette().create_cache(
            histogram_bin_count,
            histogram_max_value,
            lookup_table_count,
        );
        Self {
            fractal,
            field,
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

        // (a) Fill the field via the fractal's FieldKernel.
        compute_raw_field(
            &spec,
            self.n_max_plus_1,
            sampling_level,
            &self.fractal,
            &mut self.field,
        );

        // (b) Bin populated cells into the cache's per-color-map histograms.
        self.color_cache.reset_histograms();
        populate_histograms(
            self.n_max_plus_1,
            sampling_level,
            &self.field,
            &mut self.color_cache.histograms,
        );

        // (c) Atomically rebuild every downstream-visible cache field
        // (per-color-map CDFs, LUTs, background color) so the colorize
        // pass below can't observe a half-updated cache.
        self.color_cache
            .refresh_after_compute_pass(self.fractal.color_palette());

        // (d) Walk the output image; CDF + LUT lookup per cell; AA-average.
        colorize_collapse_unified(
            &self.color_cache,
            &self.field,
            self.n_max_plus_1,
            sampling_level,
            out,
        );
    }

    /// Re-colorize the existing field after a keyframe edit, without
    /// recomputing the fractal. Skips steps (a) and (b): the field still
    /// holds the last compute pass's raw values and the histograms still
    /// hold that pass's counts (they are only zeroed at the start of a full
    /// `render`). `refresh_after_compute_pass` therefore rebuilds identical
    /// CDFs while picking up the palette's edited keyframes and background,
    /// and step (d) re-walks the field through the refreshed LUTs.
    ///
    /// `sampling_level` must match the value the last `render` used, so the
    /// colorize pass walks the same populated sub-rectangle of the field.
    pub fn recolorize_only(&mut self, out: &mut ColorImage, sampling_level: i32) {
        debug_assert!(
            sampling_level < (self.n_max_plus_1 as i32),
            "runtime sampling_level cannot exceed the cap baked into the field buffer"
        );
        // Mirror `render`'s defensive checks: the field and output must still
        // match the fractal's spec (a resize would invalidate both, and a
        // color-only pass must never be called against a stale buffer).
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
        // (c) Rebuild CDFs (identically, from the retained histograms), LUTs
        // (from the edited keyframes), and the background color.
        self.color_cache
            .refresh_after_compute_pass(self.fractal.color_palette());

        // (d) Walk the existing field; CDF + LUT lookup per cell; AA-average.
        colorize_collapse_unified(
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

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use egui::{Color32, ColorImage};

    use crate::core::color_map::{ColorMap, ColorMapKeyFrame, ColorPalette};
    use crate::core::field_iteration::FieldKernel;
    use crate::core::image_utils::{ImageSpecification, RenderOptions, Renderable, SpeedOptimizer};

    use super::*;

    /// Minimal `Renderable` test double. `evaluate` returns a deterministic
    /// raw value for half the plane (routed through color map 0) and `None`
    /// for the rest, exercising both the colorized and background branches.
    struct TestFractal {
        image_specification: ImageSpecification,
        render_options: RenderOptions,
        palette: ColorPalette,
    }

    impl FieldKernel for TestFractal {
        fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
            if point[0] < 0.0 {
                None
            } else {
                Some((point[0] as f32, 0))
            }
        }
    }

    impl SpeedOptimizer for TestFractal {
        type ReferenceCache = RenderOptions;
        fn reference_cache(&self) -> Self::ReferenceCache {
            self.render_options
        }
        fn set_speed_optimization_level(&mut self, _level: f64, _cache: &Self::ReferenceCache) {}
    }

    impl Renderable for TestFractal {
        type Params = RenderOptions;
        fn image_specification(&self) -> &ImageSpecification {
            &self.image_specification
        }
        fn render_options(&self) -> &RenderOptions {
            &self.render_options
        }
        fn set_image_specification(&mut self, image_specification: ImageSpecification) {
            self.image_specification = image_specification;
        }
        fn write_diagnostics<W: Write>(&self, _writer: &mut W) -> io::Result<()> {
            Ok(())
        }
        fn params(&self) -> &Self::Params {
            &self.render_options
        }
        fn histogram_bin_count(&self) -> usize {
            16
        }
        fn histogram_max_value(&self) -> f32 {
            2.0
        }
        fn lookup_table_count(&self) -> usize {
            256
        }
        fn color_palette(&self) -> &ColorPalette {
            &self.palette
        }
        fn color_palette_mut(&mut self) -> &mut ColorPalette {
            &mut self.palette
        }
    }

    fn red_to_blue() -> ColorMap {
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

    fn test_pipeline() -> RenderingPipeline<TestFractal> {
        let fractal = TestFractal {
            image_specification: ImageSpecification {
                resolution: [8, 6],
                center: [0.0, 0.0],
                width: 4.0,
            },
            render_options: RenderOptions { sampling_level: 0 },
            palette: ColorPalette {
                background_color: [7, 8, 9],
                color_maps: vec![red_to_blue()],
            },
        };
        RenderingPipeline::new(fractal, 1, 16, 2.0, 256)
    }

    /// After a full `render`, `recolorize_only` with an unchanged palette must
    /// reproduce a byte-identical image: it rebuilds identical CDFs from the
    /// retained histograms and re-walks the same field.
    #[test]
    fn recolorize_only_matches_render_when_palette_unchanged() {
        let mut pipeline = test_pipeline();
        let mut rendered = ColorImage::filled([8, 6], Color32::BLACK);
        let mut recolorized = ColorImage::filled([8, 6], Color32::BLACK);

        pipeline.render(&mut rendered, 0);
        pipeline.recolorize_only(&mut recolorized, 0);

        assert_eq!(rendered.pixels, recolorized.pixels);
    }

    /// A keyframe edit followed by `recolorize_only` (no recompute) must
    /// change the output, and must match a full `render` performed after the
    /// same edit — confirming the fast path picks up palette edits correctly.
    #[test]
    fn recolorize_only_picks_up_keyframe_edits() {
        let mut pipeline = test_pipeline();
        let mut rendered = ColorImage::filled([8, 6], Color32::BLACK);
        pipeline.render(&mut rendered, 0);

        pipeline.fractal_mut().color_palette_mut().color_maps[0][0].rgb_raw = [0, 255, 0];

        let mut recolorized = ColorImage::filled([8, 6], Color32::BLACK);
        pipeline.recolorize_only(&mut recolorized, 0);
        assert_ne!(rendered.pixels, recolorized.pixels);

        let mut fully_rerendered = ColorImage::filled([8, 6], Color32::BLACK);
        pipeline.render(&mut fully_rerendered, 0);
        assert_eq!(recolorized.pixels, fully_rerendered.pixels);
    }
}
