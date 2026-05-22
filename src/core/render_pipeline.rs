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
        let outer = (spec.resolution[0] as usize) * n_max_plus_1;
        let inner = (spec.resolution[1] as usize) * n_max_plus_1;
        let field = (0..outer).map(|_| vec![None; inner]).collect();
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
