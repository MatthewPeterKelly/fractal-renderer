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

use egui::ColorImage;

use crate::core::color_map::ColorMapKind;
use crate::core::field_iteration::colorize_collapse;
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
