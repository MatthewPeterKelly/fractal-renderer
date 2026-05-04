# Phase 3 — Detailed Plan

> **Reading order:** read [gui-unification-roadmap.md](gui-unification-roadmap.md)
> §5 first for the post-Phase-3 data model. This document is the
> implementation playbook: file lists, trait shapes, commit boundaries,
> verification gates, and the JSON migration strategy.

## 1. Goal

Finish the renderer-architecture pre-work begun in Phase 2:

1. **Lift AA / block-fill iteration into core.** All three fractals
   currently duplicate the same `if sampling_level >= 0 / else` loop body
   in `compute_raw_field`, `populate_histogram`, and `normalize_field`.
   Replace with shared core helpers generic over a tiny `FieldKernel` trait.
2. **Collapse the three `ColorMapKind` variants into one `ColorMap`.**
   Mandelbrot, Julia, DDP, and Newton all use the same shape: a `flat_color`
   plus a `Vec<gradient>`. N=1 is the special case for non-Newton fractals;
   N>1 is the general case used by Newton.
3. **Drop `normalize_field`.** Move CDF lookup from a pre-pass over the
   field to the per-cell colorize step. The field stays raw end-to-end.
4. **Per-root histograms for Newton.** With N gradients, allocate N
   histograms / N CDFs / N LUTs. Each Newton root gets its own
   iteration-count distribution; Mandelbrot/Julia/DDP trivially use N=1.

After Phase 3, each fractal's `Renderable` impl boils down to:

- A 1-line `evaluate(point) -> Option<(f32, u32)>` method.
- Static config: `image_specification`, `params`, `histogram_bin_count`,
  `histogram_max_value`, `lookup_table_count`, `color_map`, plus the
  housekeeping methods.

No fractal owns any iteration logic.

## 2. Pixel-hash impact

| Family             | Hash change | Why                                                                                                                                                  |
| ------------------ | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Mandelbrot         | invariant   | N=1 → per-root histogram = single histogram. Moving CDF lookup from pre-pass to colorize-time is mathematically a no-op (same LUT, same percentile). |
| Julia              | invariant   | Same reasoning as Mandelbrot.                                                                                                                        |
| DDP                | regenerated | DDP is now histogrammed (was a no-op). Output is visually identical (gradient is constant-color), but the bit-level encoding shifts.                 |
| Newton             | regenerated | Per-root CDFs are an algorithmic improvement: each basin gets its own iteration-count distribution.                                                  |
| Barnsley/Serpinsky | invariant   | Out of scope (point-deposit path; no `Renderable` impl).                                                                                             |

The Mandelbrot/Julia invariance is the load-bearing correctness gate. **Any
hash regression on those families is a bug** — investigate before
regenerating.

## 3. Architecture

### 3.1 Trait split

```rust
// src/core/field_iteration.rs (new)

/// Domain-specific per-point evaluation. Each fractal implements exactly
/// this much of the math.
pub trait FieldKernel: Sync + Send {
    /// Evaluate the scalar field at one real-space point.
    /// Returns `Some((value, gradient_index))` or `None` for "no value".
    /// `gradient_index` indexes into the fractal's `ColorMap::gradients`.
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)>;
}
```

```rust
// src/core/image_utils.rs

/// Pipeline plumbing. Adds the things every fractal needs that aren't
/// per-point math.
pub trait Renderable: FieldKernel + SpeedOptimizer {
    type Params: Serialize + Debug;

    fn image_specification(&self) -> &ImageSpecification;
    fn set_image_specification(&mut self, image_specification: ImageSpecification);
    fn render_options(&self) -> &RenderOptions;
    fn params(&self) -> &Self::Params;
    fn write_diagnostics<W: Write>(&self, writer: &mut W) -> io::Result<()>;
    fn color_map(&self) -> &ColorMap;
    fn color_map_mut(&mut self) -> &mut ColorMap;  // used in Phase 7

    /// Histogram capacity in bins per gradient.
    fn histogram_bin_count(&self) -> usize;
    /// Maximum scalar value the histogram can absorb.
    fn histogram_max_value(&self) -> f32;
    /// LUT resolution per gradient.
    fn lookup_table_count(&self) -> usize;
}
```

The three field-iteration methods (`compute_raw_field`,
`populate_histogram`, `normalize_field`) are gone from the trait. They are
free functions in `core::field_iteration` generic over `K: FieldKernel`.

### 3.2 The unified `ColorMap` type

```rust
// src/core/color_map.rs

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorMap {
    /// Color used for cells whose evaluation produced no scalar.
    pub flat_color: [u8; 3],
    /// One gradient per "channel". Length == 1 for non-Newton; one per
    /// root for Newton. The `u32` in each cell indexes here.
    pub gradients: Vec<Vec<ColorMapKeyFrame>>,
}

pub struct ColorMapCache {
    pub cdfs: Vec<CumulativeDistributionFunction>,
    pub luts: Vec<ColorMapLookUpTable>,
    pub flat: Color32,
}

impl ColorMap {
    pub fn create_cache(
        &self,
        histogram_bin_count: usize,
        histogram_max_value: f32,
        lookup_table_count: usize,
    ) -> ColorMapCache;

    /// Refresh `flat` and `luts` from current keyframes / flat color.
    /// Does NOT touch `cdfs` (those are owned by the pipeline and
    /// refreshed from histograms after each compute pass).
    pub fn refresh_cache(&self, cache: &mut ColorMapCache);
}

#[inline]
pub fn colorize_cell(cache: &ColorMapCache, cell: Option<(f32, u32)>) -> [u8; 3] {
    match cell {
        Some((value, gradient_index)) => {
            let g = gradient_index as usize;
            let pct = cache.cdfs[g].percentile(value);
            cache.luts[g].compute_pixel(pct).0
        }
        None => [cache.flat.r(), cache.flat.g(), cache.flat.b()],
    }
}
```

The `ColorMapKind` trait is gone. With one impl, the trait carries no
weight; `ColorMap` exposes its operations as inherent methods.

### 3.3 Five-phase pipeline

```rust
// src/core/render_pipeline.rs (rewritten)

pub struct RenderingPipeline<F: Renderable> {
    fractal: F,
    field: Vec<Vec<Option<(f32, u32)>>>,
    histograms: Vec<Histogram>,
    color_cache: ColorMapCache,
    n_max_plus_1: usize,
}

impl<F: Renderable> RenderingPipeline<F> {
    pub fn render(&mut self, out: &mut ColorImage, sampling_level: i32) {
        // (a) Fill the field.
        field_iteration::compute_raw_field(
            self.fractal.image_specification(),
            self.n_max_plus_1, sampling_level, &self.fractal, &mut self.field);

        // (b) Bin into per-gradient histograms.
        for h in &mut self.histograms { h.reset(); }
        field_iteration::populate_histograms(
            self.n_max_plus_1, sampling_level, &self.field, &mut self.histograms);

        // (c) Rebuild per-gradient CDFs from histograms.
        for (cdf, hist) in self.color_cache.cdfs.iter_mut().zip(&self.histograms) {
            cdf.reset(hist);
        }

        // (d) Refresh LUTs from current keyframes.
        self.fractal.color_map().refresh_cache(&mut self.color_cache);

        // (e) Walk field; CDF + LUT lookup per cell; AA-average per output pixel.
        field_iteration::colorize_collapse(
            &self.color_cache, &self.field,
            self.n_max_plus_1, sampling_level, out);
    }
}
```

### 3.4 Core iteration helpers

Three functions, all in `src/core/field_iteration.rs`. All share a
single `for_each_populated_cell_index` traversal that knows the AA / block-fill
skip rules; the helpers thread different per-cell closures through it.

```rust
pub fn compute_raw_field<K: FieldKernel>(
    spec: &ImageSpecification,
    n_max_plus_1: usize,
    sampling_level: i32,
    kernel: &K,
    field: &mut Vec<Vec<Option<(f32, u32)>>>,
);

pub fn populate_histograms(
    n_max_plus_1: usize,
    sampling_level: i32,
    field: &[Vec<Option<(f32, u32)>>],
    histograms: &mut [Histogram],
);

pub fn colorize_collapse(
    cache: &ColorMapCache,
    field: &[Vec<Option<(f32, u32)>>],
    n_max_plus_1: usize,
    sampling_level: i32,
    out: &mut ColorImage,
);
```

`populate_histograms` takes `&mut [Histogram]` because each gradient gets
its own histogram. The cell's gradient index routes to
`histograms[gradient_index]`.

`Histogram::insert` is currently `&self` (interior mutability). Phase 3 may
keep that or switch to `&mut self`; the choice depends on whether the
parallel-iter histogram update needs lock-free atomic bins. **TBD during
implementation.**

## 4. JSON migration

A Python script `scripts/migrate_phase_3_color_maps.py` rewrites all
example/test/bench JSONs. The migration is per-fractal:

### 4.1 Mandelbrot / Julia (`BackgroundWithColorMap` → `ColorMap`)

```jsonc
// before:
"color": {
  "background": [9, 9, 9],
  "color_map": [{ "query": 0.0, "rgb_raw": [...] }, ...]
}
// after:
"color": {
  "flat_color": [9, 9, 9],
  "gradients": [
    [{ "query": 0.0, "rgb_raw": [...] }, ...]
  ]
}
```

### 4.2 DDP (`ForegroundBackground` → `ColorMap`)

```jsonc
// before:
"color": {
  "foreground": [255, 255, 255],
  "background": [0, 0, 0]
}
// after (single-keyframe constant-color gradient):
"color": {
  "flat_color": [0, 0, 0],
  "gradients": [
    [
      { "query": 0.0, "rgb_raw": [255, 255, 255] },
      { "query": 1.0, "rgb_raw": [255, 255, 255] }
    ]
  ]
}
```

DDP also gains tiny `histogram_bin_count` and `lookup_table_count` fields
(value: small integer like 4 — the histogram is unused beyond producing a
degenerate CDF for the constant gradient, so size is irrelevant).

### 4.3 Newton (`MultiColorMap` → `ColorMap`)

```jsonc
// before:
"color": {
  "cyclic_attractor": [0, 0, 0],
  "color_maps": [[...], [...], ...]
}
// after:
"color": {
  "flat_color": [0, 0, 0],
  "gradients": [[...], [...], ...]
}
```

### 4.4 DDP `#[serde(default)]` shim

Pre-Phase-1 DDP JSONs without a `color` field still need to parse. The
default returns the new `ColorMap` shape with the constant white-on-black
gradient (matching the legacy hard-coded values).

### 4.5 Test gate

`tests/example_parameter_validation_tests.rs` runs after migration to
confirm every JSON parses cleanly into the new `ColorMap` shape.

## 5. Per-fractal diff (rough)

### 5.1 `src/fractals/quadratic_map.rs`

- **Delete:** `compute_raw_field_quadratic`, `walk_populated_quadratic_cells`,
  `normalize_populated_cells`, the per-fractal `compute_raw_field` /
  `populate_histogram` / `normalize_field` impls.
- **Add:** one `impl FieldKernel for QuadraticMap<T>` with a 5-line
  `evaluate` that calls `self.fractal_params.normalized_log_escape_count(point)`
  and wraps the result as `Some((value, 0))`.
- **Slim Renderable impl:** drop the iteration methods; keep config.
- **Net:** ≈ −145 lines.

### 5.2 `src/fractals/newtons_method.rs`

- **Delete:** the inlined per-fractal compute / histogram / normalize
  blocks plus `walk_populated_newton_cells` (≈ 155 lines).
- **Add:** `impl FieldKernel for NewtonsMethodRenderable<F>` with
  `evaluate` calling the existing `newton_rhapson_iteration_sequence` +
  `root_index` and returning `Some((smooth_iter, root_index))`.
- **Slim Renderable impl:** drop iteration methods.
- **Net:** ≈ −140 lines.

### 5.3 `src/fractals/driven_damped_pendulum.rs`

- **Delete:** the inlined `compute_raw_field` AA + block-fill loops
  (≈ 68 lines).
- **Add:** `impl FieldKernel for DrivenDampedPendulumParams` with
  `evaluate` calling `compute_basin_of_attraction` and mapping to
  `Some((histogram_max_value, 0))` for in-basin or `None` for out.
  Concretely: `compute_basin_of_attraction` returns `Option<i32>`; the new
  `evaluate` only cares whether it's `Some(0)` (zeroth basin) vs anything
  else. Map zeroth basin → `Some((1.0, 0))`, else → `None`.
- DDP's `histogram_max_value` returns `1.0`; `histogram_bin_count` returns
  4 (small, since the histogram drives a constant-color gradient that
  doesn't depend on the percentile output).
- **Net:** ≈ −65 lines.

### 5.4 `src/core/color_map.rs`

- **Delete:** `ColorMapKind` trait, `ForegroundBackground`,
  `BackgroundWithColorMap`, `MultiColorMap`, three `ColorMapKind` impls.
- **Add:** `ColorMap` struct, `ColorMapCache` struct, `colorize_cell` free
  function.

### 5.5 `src/core/render_pipeline.rs`

- **Rewrite:** `RenderingPipeline<F>` to own `histograms: Vec<Histogram>`
  instead of one histogram + one CDF + one cache. Drop the `normalize_field`
  call from `render`. Move `colorize_collapse` to `field_iteration.rs`.

### 5.6 `src/core/image_utils.rs`

- **Trait:** `Renderable` no longer has `ColorMap` associated type, no
  longer has `compute_raw_field` / `populate_histogram` / `normalize_field`.
  Concrete return type from `color_map()` is `&ColorMap`. Becomes:
  `pub trait Renderable: FieldKernel + SpeedOptimizer { … }`.

### 5.7 `src/core/field_iteration.rs` (new)

Houses `FieldKernel`, `compute_raw_field`, `populate_histograms`,
`colorize_collapse`, plus extensive unit tests against synthetic kernels.

## 6. Commit structure

> **Status:** sketched. Final commit boundaries depend on what produces a
> bisectable diff at each step. Adjust during implementation if a different
> split reads better.

### Commit 3.1 — `FieldKernel` + core helpers, parallel-to-old

- Add `src/core/field_iteration.rs` with `FieldKernel`,
  `compute_raw_field`, `populate_histograms` (plural), and
  `colorize_collapse` moved from `render_pipeline.rs`.
- Old per-fractal `compute_raw_field` etc. retained.
- Unit tests against synthetic kernels.
- Pixel-hash regression tests still pass (nothing called the new helpers
  yet).

### Commit 3.2 — Migrate fractals to `FieldKernel`; delete duplicates

- Each fractal implements `FieldKernel`.
- Old per-fractal iteration methods deleted.
- The `Renderable` trait still has `ColorMap = ColorMapKind` impls
  (Phase 2 shape), but the iteration methods are gone — pipeline uses the
  new core helpers.
- Pixel hashes stay invariant (still using current `ColorMapKind` machinery
  - per-cell `apply_cdf` etc.) — wait no: at this point we still have
    `normalize_field` calls? Need to either (a) keep `normalize_field` on the
    trait and call it from the pipeline, deleting it in 3.3; or (b) drop
    it here and route through `colorize_cell` in 3.3 atomically.
    **Decision deferred to implementation:** whichever produces a smaller
    bisectable diff.

### Commit 3.3 — Collapse `ColorMapKind`; drop normalize_field; per-root histograms

- Drop `ForegroundBackground`, `BackgroundWithColorMap`, `MultiColorMap`,
  `ColorMapKind`. Add `ColorMap`, `ColorMapCache`, `colorize_cell`.
- `RenderingPipeline` gains `histograms: Vec<Histogram>`. Drop the
  `normalize_field` call.
- All fractals migrate to the unified `ColorMap`; per-fractal `color_map()`
  returns `&ColorMap`.
- Mass JSON migration via `scripts/migrate_phase_3_color_maps.py`.
- DDP and Newton pixel hashes regenerated (with eyeball verification).
- Mandelbrot/Julia hashes invariant (gate).

## 7. Verification

### 7.1 CI gates per commit

- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`,
  `cargo bench --no-run`, `npm run fmt:check`.

### 7.2 Pixel-hash gates

- 3.1: all hashes invariant.
- 3.2: all hashes invariant.
- 3.3: Mandelbrot/Julia/Barnsley/Serpinsky invariant (gate); DDP/Newton
  regenerated. Manual eyeball verification required before regenerating
  Newton hashes (see §11 risks).

### 7.3 Manual smoke tests

- `cargo run -- explore <each-fractal>` after 3.3.
- `cargo run -- render <each-example>` after 3.3.
- One Newton render per system type (`roots_of_unity`, `cosh_minus_one`)
  with at least 3 roots to exercise per-root histograms.

## 8. Out of scope for Phase 3

- `chaos_game` (Barnsley fern, Sierpinski) — different rendering path
  (point-deposit). Untouched.
- Editor UI work — Phase 5+.
- `recolorize_only` fast path — Phase 7.
- The Phase-3 architecture _enables_ Phase 7 by keeping the field raw and
  the per-gradient CDFs in the cache, but Phase 3 itself doesn't add the
  dirty-flag plumbing.
- Changes to `RenderOptions::sampling_level`, the speed regulator, or the
  field-allocation strategy. All of those are Phase-2 territory and stay
  as-is.

## 9. Open questions for the implementer

1. **Histogram interior mutability.** Decided: keep the existing
   `Histogram::insert(&self, value)` atomic-bins shape. Each cell routes to
   one of N histograms based on its gradient index — parallel-iter pattern
   is unchanged. Benchmark only if atomic contention surfaces.
2. **DDP's `histogram_bin_count`.** Set to `1` (the canonical positive
   integer). The histogram and CDF are decorative — DDP's gradient is
   constant-color, so the percentile output never affects pixels. Verify
   `Histogram::new(1, max_value)` and `CumulativeDistributionFunction::new`
   handle `bin_count=1` cleanly during implementation; if there's a
   div-by-zero or off-by-one in the histogram math at N=1, fix the latent
   bug rather than working around it.
3. **`gradients.len() == 0` validation.** Reject at `ColorMap`
   deserialization with a structured serde error, _or_ at
   `RenderingPipeline::new` with a runtime panic? Recommended: serde
   rejection — keeps the runtime free of "what if zero gradients?" checks.
4. **Drop the `QuadraticMap<T>` wrapper struct.** After Phase 2 it just
   contains `fractal_params: T`. Phase 3 is a natural time to fold it
   away — implement `Renderable` / `FieldKernel` directly on
   `T: QuadraticMapParams + Sync + Send` and delete the wrapper.

## 10. Files touched (summary)

**Modified:**

- `src/core/color_map.rs`
- `src/core/render_pipeline.rs`
- `src/core/image_utils.rs`
- `src/core/render_window.rs`
- `src/fractals/quadratic_map.rs`
- `src/fractals/newtons_method.rs`
- `src/fractals/driven_damped_pendulum.rs`
- `benches/benchmark.rs` (if any iteration-method calls are visible)
- `examples/common/mod.rs` (if it constructs the deleted color-map types)
- All `**/*.json` under `examples/`, `benches/`, `tests/param_files/`
  via the migration script.
- `tests/full_cli_integration_and_regression_tests.rs` (regenerated
  hashes for DDP/Newton).
- `docs/gui-unification-roadmap.md` (mark Phase 3 done post-merge).

**Created:**

- `src/core/field_iteration.rs`
- `scripts/migrate_phase_3_color_maps.py`

**Deleted:**

- None at the file level; deletions are within `src/core/color_map.rs`.

## 11. Risks

See §11 of the main roadmap. The two Phase-3-specific risks are:

- **Mandelbrot/Julia hash regression.** Treated as a bug (the refactor
  is mathematically a no-op for N=1). If hashes shift, the most likely
  cause is an off-by-one in the `cdf.percentile` call or in LUT
  construction.
- **Newton output structurally wrong.** Per-root histograms are a real
  algorithmic change; the intended visual result is per-basin contrast
  improvement, not loss of structure. Eyeball-verify 2-3 PNGs per Newton
  system type before regenerating hashes.
