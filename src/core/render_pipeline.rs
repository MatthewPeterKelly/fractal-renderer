//! Top-level orchestrator that owns all reusable buffers and drives the
//! five-phase render pipeline:
//!
//! 1. (a) `field_iteration::compute_raw_field` — fill the field with raw
//!    `Option<(f32, u32)>` cells via the fractal's `FieldKernel::evaluate`.
//! 2. (b) `field_iteration::populate_histograms` — bin populated cells into
//!    the per-gradient histograms.
//! 3. (c) Rebuild each per-gradient `CumulativeDistributionFunction` from
//!    the freshly-populated histogram.
//! 4. (d) `ColorMap::refresh_cache` — refresh per-gradient LUTs and the
//!    flat color from the current keyframes.
//! 5. (e) `field_iteration::colorize_collapse_unified` — walk the output
//!    `egui::ColorImage`, averaging `(n+1)²` subpixel `[u8; 3]` results
//!    into each output pixel via `colorize_cell`.
//!
//! All buffers are allocated once at construction (or `resize`); per-frame
//! and per-pixel allocations are zero. Dispatch is fully monomorphized over
//! `F: Renderable`; there is no `dyn` or runtime variant matching on the hot
//! path. The field stays raw end-to-end — there is no `normalize_field`
//! step; CDF lookup happens inside `colorize_cell` at colorize time.

use egui::ColorImage;

use crate::core::color_map::ColorMapCache;
use crate::core::field_iteration::{
    colorize_collapse_unified, compute_raw_field, populate_histograms,
};
use crate::core::histogram::Histogram;
use crate::core::image_utils::Renderable;

/// Top-level orchestrator that owns all reusable buffers for one fractal
/// instance and runs the five-phase pipeline against them on every render.
pub struct RenderingPipeline<F: Renderable> {
    /// The fractal whose `FieldKernel::evaluate` drives the compute phase.
    fractal: F,
    /// Subpixel field, sized at construction for `(n_max+1)·W × (n_max+1)·H`
    /// where `n_max+1` is derived from the user's JSON `sampling_level`.
    field: Vec<Vec<Option<(f32, u32)>>>,
    /// One histogram per gradient. Length matches
    /// `fractal.color_map().gradients.len()`.
    histograms: Vec<Histogram>,
    /// Allocation-once color cache (per-gradient CDFs + LUTs and the
    /// pre-converted flat `Color32`). CDFs are refreshed by the pipeline
    /// after each compute pass; LUTs are refreshed by
    /// `ColorMap::refresh_cache` from current keyframes.
    color_cache: ColorMapCache,
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
        let histograms = fractal
            .color_map()
            .gradients
            .iter()
            .map(|_| Histogram::new(histogram_bin_count, histogram_max_value))
            .collect();
        let color_cache = fractal.color_map().create_cache(
            histogram_bin_count,
            histogram_max_value,
            lookup_table_count,
        );
        Self {
            fractal,
            field,
            histograms,
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

        // (b) Bin populated cells into per-gradient histograms.
        for h in &mut self.histograms {
            h.reset();
        }
        populate_histograms(
            self.n_max_plus_1,
            sampling_level,
            &self.field,
            &mut self.histograms,
        );

        // (c) Rebuild per-gradient CDFs from the freshly-populated histograms.
        for (cdf, hist) in self.color_cache.cdfs.iter_mut().zip(&self.histograms) {
            cdf.reset(hist);
        }

        // (d) Refresh LUTs and flat color from current keyframes.
        self.fractal
            .color_map()
            .refresh_cache(&mut self.color_cache);

        // (e) Walk the output image; CDF + LUT lookup per cell; AA-average.
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
