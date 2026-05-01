# Phase 2 Detailed Plan — Compute / Color Split

This document is the implementer-facing plan for Phase 2 of the GUI unification
roadmap. It expands on §6 Phase 2 of [gui-unification-roadmap.md](gui-unification-roadmap.md)
with concrete trait shapes, file lists, commit boundaries, and verification
gates. Read the parent roadmap first for vision and cross-phase context.

**Audience:** the agent or contributor implementing Phase 2. Self-contained;
no prior conversation context needed.

---

## 1. Goal

Factor `Renderable` so per-pixel scalar computation is cleanly separated from
colorization. Hoist histogram, CDF, and lookup-table state into a single
top-level `RenderingPipeline<F>` orchestrator owned by `PixelGrid`. Replace the
two-axis `(subpixel_antialiasing, downsample_stride)` knob with a single
signed `sampling_level: i32`. Land an intentional pixel-hash bump (Mandelbrot,
Julia, Newton) when the histogram switches from a sub-sample grid to the full
field — DDP unaffected.

The result: a single render path, allocation-free per frame, statically
dispatched, with re-colorize-only as a free byproduct of the cache layout
(Phase 6 just adds the dirty flags).

---

## 2. The pipeline (four in-place phases)

`RenderingPipeline::render(out, sampling_level)` runs four phases against a
single set of preallocated buffers:

1. **(a) Compute raw field.** `Renderable::compute_raw_field(sampling_level, &mut field)`
   fills the field buffer in place with raw, un-normalized values. Stride
   determines which cells get populated; the rest stay as garbage from the
   previous render and are skipped by later stages.

2. **(b) Populate histogram.** `Renderable::populate_histogram(sampling_level, &field, &histogram)`
   walks the populated cells of the field (skipping the cells the stride
   left untouched) and inserts each value into the histogram. No-op for
   fractals that don't normalize (DDP).

3. **(c) CDF normalize the field.** `cdf.reset(&histogram)` rebuilds the CDF
   in place, then `Renderable::normalize_field(sampling_level, &cdf, &mut field)`
   replaces each populated cell's raw value with `cdf.percentile(value)` ∈
   [0, 1]. No-op for DDP.

4. **(d) Refresh color cache + colorize-collapse.**
   `color_map.refresh_cache(&mut color_cache)` rebuilds lookup tables
   in place from current keyframes (allocation-free; Phase 6 will gate this
   on a dirty flag). Then a generic `colorize_collapse::<F::ColorMap>(...)`
   walks the output `egui::ColorImage` row-major; for each output pixel it
   reads the corresponding (n+1)² block of the field, looks up each
   subpixel's `[u8; 3]` via `ColorMapKind::colorize_cell`, and averages.

The CDF (b/c) is a property of the field, not the keyframes — so editing
keyframes invalidates only the color cache (cheap rebuild), not the CDF or the
field. That's what makes Phase 6 re-colorize-only correct.

---

## 3. Trait shapes

### 3.1 `ColorMapKind`

```rust
/// A color-map shape paired with its per-cell field type and a cached form
/// suitable for the colorize hot path.
pub trait ColorMapKind: Sized {
    /// Per-(sub)pixel value the color map consumes. CDF-normalized for the
    /// scalar variants (Phase 2 invariant: cells handed to `colorize_cell`
    /// have already been through `Renderable::normalize_field`).
    type Cell: Copy + Send + Sync;

    /// Allocation-once cache holding lookup tables and pre-converted `Color32`
    /// flat colors. Mutated in place by `refresh_cache`.
    type Cache: Send + Sync;

    /// One-time allocation at pipeline construction. `lookup_table_count`
    /// applies to variants that hold one or more `ColorMapLookUpTable`s.
    fn create_cache(&self, lookup_table_count: usize) -> Self::Cache;

    /// In-place rebuild of the cache from current keyframes / flat colors.
    /// Allocation-free. Called once at startup and again whenever keyframes
    /// change (Phase 6).
    fn refresh_cache(&self, cache: &mut Self::Cache);

    /// Per-cell color lookup. Statically dispatched; called inside the
    /// AA-collapse loop. No allocation, no `dyn`.
    fn colorize_cell(cache: &Self::Cache, cell: Self::Cell) -> [u8; 3];
}
```

### 3.2 Per-variant types

| `ColorMapKind` impl      | `Cell`               | `Cache`                                                                   |
| ------------------------ | -------------------- | ------------------------------------------------------------------------- |
| `ForegroundBackground`   | `Option<i32>`        | `(Color32, Color32)` (foreground, background)                             |
| `BackgroundWithColorMap` | `Option<f32>`        | `(ColorMapLookUpTable, Color32)` (table, background)                      |
| `MultiColorMap`          | `Option<(f32, u32)>` | `(Vec<ColorMapLookUpTable>, Color32)` (per-root tables, cyclic_attractor) |

`MultiColorMap`'s `Vec<ColorMapLookUpTable>` is allocated once at
`create_cache` time, sized to `color_maps.len()`. `refresh_cache` calls
`reset` on each table in place. The number of roots is fixed by the input
JSON and does not change during a session, so the outer `Vec` length is
stable.

### 3.3 `Renderable` (revised)

```rust
pub trait Renderable: Sync + Send + SpeedOptimizer {
    type Params: Serialize + Debug;
    type ColorMap: ColorMapKind;

    /// (a) Fill the preallocated `field` buffer with raw, un-normalized
    /// values. `sampling_level` controls stride per §4 below.
    fn compute_raw_field(
        &self,
        sampling_level: i32,
        field: &mut Vec<Vec<<Self::ColorMap as ColorMapKind>::Cell>>,
    );

    /// (b) Walk the populated cells of `field` and insert into `histogram`.
    /// `histogram.reset()` is called by the pipeline before this; no need
    /// to reset here. Default impl is a no-op (DDP).
    fn populate_histogram(
        &self,
        _sampling_level: i32,
        _field: &[Vec<<Self::ColorMap as ColorMapKind>::Cell>],
        _histogram: &Histogram,
    ) {}

    /// (c) Replace each populated cell's raw value with its CDF percentile,
    /// in place. Default impl is a no-op (DDP).
    fn normalize_field(
        &self,
        _sampling_level: i32,
        _cdf: &CumulativeDistributionFunction,
        _field: &mut Vec<Vec<<Self::ColorMap as ColorMapKind>::Cell>>,
    ) {}

    fn color_map(&self) -> &Self::ColorMap;

    fn image_specification(&self) -> &ImageSpecification;
    fn render_options(&self) -> &RenderOptions;
    fn set_image_specification(&mut self, image_specification: ImageSpecification);
    fn write_diagnostics<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()>;
    fn params(&self) -> &Self::Params;
}
```

`render_point`, `render_to_buffer`, the default body of `render_to_color_image`,
and `wrap_renderer_with_antialiasing` are all gone after commit 2.2.

### 3.4 `RenderingPipeline<F>`

```rust
pub struct RenderingPipeline<F: Renderable> {
    fractal: F,
    /// Subpixel field, sized once at construction for `(n_max+1)·W × (n_max+1)·H`,
    /// where `n_max+1 = max_sampling_level_magnitude + 1` derived from
    /// `fractal.render_options().sampling_level.abs()` (a cap from the params).
    field: Vec<Vec<<F::ColorMap as ColorMapKind>::Cell>>,
    /// Histogram + CDF, allocated once. `compute_field`'s histogram pass walks
    /// the field; CDF rebuilds in place after each.
    histogram: Histogram,
    cdf: CumulativeDistributionFunction,
    /// Color cache holding lookup tables / flat Color32s, allocated once.
    color_cache: <F::ColorMap as ColorMapKind>::Cache,
    /// `n_max+1`: the upsampling factor permanently applied to the field.
    /// Driven by the user's `sampling_level` JSON value (positive AA values).
    n_max_plus_1: usize,
}

impl<F: Renderable> RenderingPipeline<F> {
    pub fn new(fractal: F) -> Self { /* one-time allocation */ }

    pub fn render(&mut self, out: &mut egui::ColorImage, sampling_level: i32) {
        self.fractal.compute_raw_field(sampling_level, &mut self.field);
        self.histogram.reset();
        self.fractal.populate_histogram(sampling_level, &self.field, &self.histogram);
        self.cdf.reset(&self.histogram);
        self.fractal.normalize_field(sampling_level, &self.cdf, &mut self.field);
        self.fractal.color_map().refresh_cache(&mut self.color_cache);
        colorize_collapse::<F::ColorMap>(
            &self.color_cache, &self.field,
            self.n_max_plus_1, sampling_level, out,
        );
    }

    pub fn resize(&mut self, image_specification: ImageSpecification) {
        // Reallocate `field` and `out` to match the new resolution.
        // Histogram/CDF/color_cache sizes are independent of resolution.
    }

    pub fn fractal(&self) -> &F { &self.fractal }
    pub fn fractal_mut(&mut self) -> &mut F { &mut self.fractal }
}
```

### 3.5 `colorize_collapse` — the generic AA-loop

```rust
pub fn colorize_collapse<C: ColorMapKind>(
    cache: &C::Cache,
    field: &[Vec<C::Cell>],
    n_max_plus_1: usize,
    sampling_level: i32,
    out: &mut egui::ColorImage,
);
```

Internally:

- Compute effective stride from `sampling_level` per §4.
- For positive `sampling_level` (AA), iterate output pixels; per pixel sum
  `(sampling_level+1)²` subpixel `[u8;3]` results from `C::colorize_cell` and
  divide. Reads only cells inside the populated stride.
- For zero `sampling_level`, one cell per output pixel, no averaging.
- For negative `sampling_level` (block-fill), one cell per `(|sampling_level|+1)²`
  output pixels; fill the block with the same color.

Per-pixel hot path: zero allocation, zero `dyn`, fully monomorphized.

---

## 4. The `sampling_level` model

`RenderOptions` collapses `subpixel_antialiasing: u32` and
`downsample_stride: usize` into a single `sampling_level: i32`:

| `sampling_level` | Field cells populated per output pixel | Output pixels per field cell | Mode                    |
| ---------------- | -------------------------------------- | ---------------------------- | ----------------------- |
| `+n` (n > 0)     | `(n+1)²`                               | 1                            | Anti-alias              |
| `0`              | 1                                      | 1                            | Baseline                |
| `−n` (n > 0)     | `1 / (n+1)²` (sparse)                  | `(n+1)²`                     | Block-fill (downsample) |

The `RenderOptions::sampling_level` value in the params JSON is the **maximum**
the pipeline ever uses — the field buffer is sized to accommodate it. The
`AdaptiveOptimizationRegulator` drives the **runtime** value passed to
`RenderingPipeline::render`:

- Level 0 (full quality, idle): runtime equals user's JSON value. Full AA
  if positive; full downsample if negative; baseline if zero.
- Level → 1.0 (interactive): runtime drops toward 0 (then negative if
  the regulator wants to push compute below baseline for interactivity).

The regulator's `set_speed_optimization_level` mutates `sampling_level`
exactly as it mutates `subpixel_antialiasing` and `downsample_stride` today
— but on a single combined axis.

**Block-fill is nearest-neighbor / zero-order hold.** Today's bilinear
interpolation between sparse samples (`KeyframeLinearPixelInerpolation` in
[src/core/image_utils.rs](../src/core/image_utils.rs)) is dropped. The user
sees `(n+1)²`-pixel blocks while the regulator is in block-fill mode; the
moment quality climbs back to baseline, full-resolution rendering resumes.
This change is intentional and acknowledged: it changes the pixel hash for
the one test fixture using `downsample_stride: 4`
([tests/param_files/mandelbrot/downsample_interpolation_regression_test.json](../tests/param_files/mandelbrot/downsample_interpolation_regression_test.json)),
which gets a regenerated hash in commit 2.3.

---

## 5. Allocation strategy

All buffers are allocated **once per session** (or per window resize) on
`RenderingPipeline::new` / `resize`, never per frame.

- **Field buffer:** `Vec<Vec<Cell>>`, sized to
  `[(n_max+1) · W][(n_max+1) · H]`. For 1080p × AA=3 (max): ~32 MB if
  `Cell = Option<f32>` (assuming 8 B per Option). For 4K × AA=3: ~125 MB.
  Acceptable; users running AA=3 already have headroom. Reallocated only on
  window resize.
- **Histogram:** `Vec<AtomicUsize>` of `histogram_bin_count` entries (~256
  by default). Reset in place per render via `Histogram::reset()`.
- **CDF:** rebuilt from histogram via `CumulativeDistributionFunction::reset`,
  which is in-place (verify this — if `CDF::reset` allocates, fix it as
  part of 2.1).
- **Color cache:** sized once at startup based on `lookup_table_count` and
  (for `MultiColorMap`) `color_maps.len()`. Refreshed in place.
- **Output `egui::ColorImage`:** owned by `PixelGrid`, sized to
  `[W, H]`. Reallocated only on resize.

Per-frame allocations: zero. Per-pixel/subpixel allocations: zero.

---

## 6. Per-fractal phase-by-phase concrete behavior

### DDP

- Cell: `Option<i32>` (basin index or non-converged).
- (a) `compute_raw_field`: parallel iterate populated cells, call
  `compute_basin_of_attraction` per (sub)pixel, store `Option<i32>`.
- (b) `populate_histogram`: default no-op.
- (c) `normalize_field`: default no-op.
- (d) `colorize_cell(cache, Some(0))` → `cache.0` (foreground); anything
  else → `cache.1` (background).

### QuadraticMap (Mandelbrot, Julia)

- Cell: `Option<f32>`.
- (a) `compute_raw_field`: parallel iterate populated cells, call
  `normalized_log_escape_count` per (sub)pixel, store raw `Option<f32>`.
- (b) `populate_histogram`: walk populated cells; `histogram.insert(value)`
  for each `Some`. **This is the histogram-source change** — today's
  `populate_histogram` (in `src/fractals/utilities.rs`) samples a
  sub-sample grid; the new path walks the actual rendered field.
- (c) `normalize_field`: walk populated cells; replace each `Some(v)` with
  `Some(cdf.percentile(v))`.
- (d) `colorize_cell(cache, None)` → background; `colorize_cell(cache, Some(p))` →
  `cache.0.lookup(p)` (where `cache.0: ColorMapLookUpTable`, indexed
  over [0, 1]).

### Newton

- Cell: `Option<(f32, u32)>` — smooth iteration count + root index.
- (a) `compute_raw_field`: parallel iterate populated cells, run
  Newton-Raphson, store raw `Option<(smooth_iter, root_index as u32)>`.
- (b) `populate_histogram`: walk populated cells; `histogram.insert(smooth)`.
- (c) `normalize_field`: walk populated cells; replace `(s, k)` with
  `(cdf.percentile(s), k)`.
- (d) `colorize_cell(cache, None)` → cyclic-attractor color;
  `colorize_cell(cache, Some((p, k)))` → `cache.0[k as usize].lookup(p)`.

---

## 7. Commit split (one PR, three commits)

### Commit 2.1 — Add new machinery, run parallel-to-old

**Goal:** the new pipeline exists end-to-end and is bit-equivalent to the old
path on representative example JSONs at the user's specified `sampling_level`
(positive only — old downsample behavior unaffected because the old path is
still production). Old runtime paths untouched.

**Files touched:**

- [src/core/color_map.rs](../src/core/color_map.rs):
  - Add `ColorMapKind` trait.
  - Add `ForegroundBackground::Cell/Cache` impls.
  - Add `BackgroundWithColorMap::Cell/Cache` impls.
  - Add `MultiColorMap::Cell/Cache` impls.
  - Unit tests (per §10.1 of the parent roadmap):
    - `colorize_cell` correctness for each variant: `None` cell, in-set
      values at 0.0 and 1.0, mid-gradient values, all entries of
      `MultiColorMap`'s `color_maps`.
    - `refresh_cache` allocation-free smoke test (call twice; assert no
      growth in the inner `Vec` capacity).
- [src/core/image_utils.rs](../src/core/image_utils.rs):
  - Extend `Renderable` trait with `type ColorMap`, `compute_raw_field`,
    `populate_histogram` (default empty), `normalize_field` (default
    empty), `color_map`. Keep all existing methods including `render_point`
    and `render_to_buffer` for now.
- [src/core/render_pipeline.rs](../src/core/render_pipeline.rs) (new file):
  - Define `RenderingPipeline<F>` struct.
  - Define `colorize_collapse::<C>` generic free function.
  - Constructor allocates field/histogram/CDF/color_cache once.
  - `render(out, sampling_level)` orchestrates the four phases.
  - Unit test for `colorize_collapse` AA averaging: synthetic 4×4 field of
    known cells, n=2, asserts averaged output matches hand-computed values.
- [src/core/mod.rs](../src/core/mod.rs): `pub mod render_pipeline;`.
- [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs):
  - Implement `compute_raw_field` (raw escape counts to populated cells),
    `populate_histogram` (sub-sample grid for now — matches old behavior),
    `normalize_field` (apply CDF percentile), `color_map` (returns
    `&self.fractal_params.color_map_params().color`).
  - Rename `QuadraticMapParams::color_map` → `color_map_params` (and the
    `_mut` version), per Phase 2 §F. Touches `mandelbrot.rs`, `julia.rs`,
    `benches/benchmark.rs`.
- [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs):
  - Implement `compute_raw_field` (basin indices); `color_map` returns
    `&self.color`. `populate_histogram` and `normalize_field` use the
    default empty impls.
- [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs):
  - Implement `compute_raw_field` (smooth iter + root index),
    `populate_histogram`, `normalize_field`, `color_map` (returns
    `&self.params.color`).
- [tests/](../tests/) — new `phase_2_pixel_equivalence_tests.rs`:
  - Pick a small AA=0 Mandelbrot example, an AA=2 Mandelbrot example, a
    DDP example, a Newton example.
  - For each: render through old `Renderable::render_to_buffer` →
    `display_buffer_to_color_image`. Render through `RenderingPipeline::new(...)
.render(out, sampling_level)`. Assert pixel-by-pixel `Color32` equality.
  - This is the gate for 2.1.

**Verification:**

- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo bench --no-run`.
- Existing pixel-hash regression tests pass unchanged (production path
  hasn't moved).
- New unit + equivalence tests pass.

### Commit 2.2 — Switch runtime paths, delete legacy, JSON migration

**Goal:** the production render path goes through `RenderingPipeline`. All
legacy compute/render code is deleted. JSONs migrated to `sampling_level`.
Pixel-hash regression suite re-verified — should still pass at
`sampling_level == subpixel_antialiasing` (positive-only) because 2.1
established bit equivalence at full quality. (The one fixture using
`downsample_stride: 4` will produce different pixels because block-fill ≠
linear interpolation; its hash gets regenerated as part of this commit.)

**Files touched:**

- [src/core/image_utils.rs](../src/core/image_utils.rs):
  - Delete from `Renderable`: `render_point`, default `render_to_buffer`.
  - Delete free functions: `generate_scalar_image`,
    `generate_scalar_image_in_place`, `wrap_renderer_with_antialiasing`,
    `KeyframeLinearPixelInerpolation`, `render_single_row_within_image`,
    `fill_skipped_entries`, `PixelRenderLambda` trait, `SubpixelGridMask`
    if unused after the dust settles.
  - `RenderOptions`:
    - Replace `subpixel_antialiasing: u32` and `downsample_stride: usize`
      with `sampling_level: i32`.
    - Update `SpeedOptimizer for RenderOptions::set_speed_optimization_level`
      to mutate `sampling_level` along the unified axis.
  - Rewrite `render(renderable, file_prefix)` to use `RenderingPipeline`:
    construct, allocate `egui::ColorImage` at full resolution, call
    `pipeline.render(&mut image, renderable.render_options().sampling_level)`,
    convert `ColorImage::pixels` → `image::ImageBuffer<Rgb<u8>, _>` for PNG
    write.
- [src/core/render_window.rs](../src/core/render_window.rs):
  - `PixelGrid<F>` now wraps `Arc<Mutex<RenderingPipeline<F>>>` instead of
    `Arc<Mutex<F>>` plus `display_buffer`. The `display_buffer` becomes a
    `Mutex<egui::ColorImage>`.
  - `PixelGrid::render` (background thread) calls
    `pipeline.lock().unwrap().render(&mut color_image, sampling_level)`.
  - `display_buffer_to_color_image` deleted (no transposition needed).
  - `draw` copies the `ColorImage` into the eframe-supplied output buffer.
  - `render_to_file` writes the `ColorImage` directly (with `Color32` →
    `Rgb<u8>` conversion).
- [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs):
  - Drop `render_to_buffer` override and the `render_point` impl.
- [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs):
  - `QuadraticMap<T>` slims to `{ fractal_params: T }`. Drop
    `histogram`, `cdf`, `color_map: ColorMapLookUpTable`, `inner_color_map`,
    `background_color`. (Decision point: drop the `QuadraticMap<T>` wrapper
    entirely and impl `Renderable` directly on `T: QuadraticMapParams`?
    Probably yes; less indirection. Touch points: `image_utils::render`,
    Newton's `explore_fractal`, the `cli/explore.rs` dispatch.)
- [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs):
  - `NewtonsMethodRenderable<F>` drops `histogram`, `cdf`,
    `inner_color_maps`, `color_maps`. Becomes `{ params, system }`.
  - `update_color_map` deleted.
- [src/fractals/utilities.rs](../src/fractals/utilities.rs):
  - `populate_histogram` and `reset_color_map_lookup_table_from_cdf` retained
    if used by the new path or by benchmarks. Otherwise deleted in 2.3.
- [src/cli/explore.rs](../src/cli/explore.rs) and Newton's `explore_fractal`:
  - Construct `RenderingPipeline` instead of bare renderer. `PixelGrid::new`
    signature changes accordingly.

**JSON migration script** (run as part of 2.2):

```python
# scripts/migrate_phase_2_render_options.py — one-time mass edit.
# For every *.json under examples/, tests/param_files/, benches/:
#   - Find the `render_options` block.
#   - subpixel_antialiasing: u, downsample_stride: 1
#       → sampling_level: u   (drop both old fields)
#   - subpixel_antialiasing: 0, downsample_stride: m  (m > 1)
#       → sampling_level: -(m - 1)
#   - subpixel_antialiasing: 0, downsample_stride: 1
#       → sampling_level: 0   (drop both old fields, or omit and rely on default)
#   - Anything else: print warning and skip.
```

There's exactly one JSON with `downsample_stride > 1`
([tests/param_files/mandelbrot/downsample_interpolation_regression_test.json](../tests/param_files/mandelbrot/downsample_interpolation_regression_test.json));
its hash in [tests/full_cli_integration_and_regression_tests.rs](../tests/full_cli_integration_and_regression_tests.rs)
will change because block-fill ≠ linear interpolation. Regenerate that hash
in this commit; spot-check the rendered image for visual sensibility.

**Verification:**

- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` — pixel-hash
  regression tests pass unchanged for AA-only fixtures (Mandelbrot AA=3, DDP
  AA=2). The one downsample fixture has its hash regenerated.
- `npm run fmt:check` after JSON migration.
- Manual smoke-test: `cargo run --release -- explore <example>` for a
  Mandelbrot, Julia, DDP, and Newton example each — same pan/zoom/save
  behavior as before.
- `cargo bench` (informational) — `populate_histogram` benchmark might
  need refactoring; address in 2.3.

### Commit 2.3 — Histogram from full field

**Goal:** flip `Renderable::populate_histogram` from sub-sample grid to
full-field walk for Mandelbrot/Julia/Newton. Drop the now-unused
`histogram_sample_count` field. Regenerate pixel-hash test fixtures for
those three families. DDP unchanged.

**Files touched:**

- [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs):
  - `populate_histogram` walks the populated cells of `field` (using the
    same stride logic as `compute_raw_field`), calling
    `histogram.insert(value)` per `Some`.
  - Drop `histogram_sample_count` from `ColorMapParams`. (`lookup_table_count`
    and `histogram_bin_count` retained — both still used.)
- [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs):
  - Same: full-field histogram, drop `histogram_sample_count` from
    `CommonParams`.
- [src/fractals/utilities.rs](../src/fractals/utilities.rs):
  - `populate_histogram` (sub-sample grid version) deleted if no caller
    remains. `reset_color_map_lookup_table_from_cdf` deleted similarly.
- [benches/benchmark.rs](../benches/benchmark.rs):
  - Refactor to bench `RenderingPipeline::render` end-to-end (or extract a
    helper that runs phases (a)+(b) only). Update `MandelbrotParams` field
    references to drop `histogram_sample_count`.
- [tests/full_cli_integration_and_regression_tests.rs](../tests/full_cli_integration_and_regression_tests.rs):
  - Regenerate expected hashes for Mandelbrot, Julia, Newton tests. DDP
    hashes unchanged.
- All JSONs under [examples/](../examples/), [tests/param_files/](../tests/param_files/),
  [benches/](../benches/) referencing `histogram_sample_count`: extra field
  silently ignored by serde (no migration needed; the field is dead). Could
  be cleaned up in a follow-up commit, optional.

**Hash regeneration procedure:**

1. Run `cargo test 2>&1 | grep -E "Hash mismatch|FAILED"` to enumerate
   broken fixtures.
2. For each: `cargo run --release -- render <fixture.json>`, eyeball the
   resulting PNG against the previous output. Should look very similar —
   slight color-distribution shift, no structural changes.
3. For any fixture where the diff looks wrong, debug before regenerating.
4. Update the expected hash strings in the test file.

**Verification:**

- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`,
  `cargo bench --no-run`, `npm run fmt:check`.
- All pixel-hash regression tests green at the new hashes.
- Spot-checked PNGs look correct (manual gate).

---

## 8. Open implementation considerations

These don't block 2.1 but should be decided before they bite:

- **Drop the `QuadraticMap<T>` wrapper?** After 2.2 it'd contain only
  `fractal_params: T`. Cleaner to impl `Renderable` directly on `T:
QuadraticMapParams + ...`. Touches the call sites in `cli/explore.rs` and
  `fractals/newtons_method.rs::explore_fractal` and the offline `render`
  function. Recommended yes; keep small unless the diff sprawls.
- **`CDF::reset` allocation audit.** If today's
  [src/core/histogram.rs](../src/core/histogram.rs) `CumulativeDistributionFunction::reset`
  reallocates internal vectors, fix it to be allocation-free in 2.1 (a
  prerequisite for the "no allocation per render" rule). One-time check
  during 2.1 implementation.
- **Field initialization on first render.** Cells outside the populated
  stride are uninitialized at the moment `populate_histogram` walks the
  field. The walk skips them (it uses the same stride as compute), so this
  is safe — but the Vec needs to be initialized once at construction
  (probably to `None` for the `Option<...>` inner type). A single
  `vec![vec![Default::default(); h]; w]` at `RenderingPipeline::new` does
  it.
- **Window resize handling.** When the window resizes mid-session,
  `RenderingPipeline::resize` reallocates the field. Hook this into
  `PixelGrid::update`'s view_control resize path. Simple but make sure
  there's no missing-resize bug at AA boundaries.
- **`SpeedOptimizer` reference-cache type for the unified axis.** Today
  `RenderOptions::ReferenceCache = RenderOptions` and it's interpolated
  per-axis. With one axis, the regulator interpolates one signed integer
  toward the user's value at level 0 and toward 0 (or some interactive
  default) at level 1. Decide the curve shape — likely a simple linear
  interpolation `runtime = level == 1.0 ? 0 : user_value`, with smoothing
  if needed.
- **Phase 6 readiness.** With this pipeline shape, Phase 6 adds:
  (1) a `keyframes_dirty: AtomicBool` flag set by editor mutations, gating
  `refresh_cache` in `render`,
  (2) a `field_dirty: AtomicBool` flag distinct from the existing render
  triggers, distinguishing "viewport changed → re-run (a)+(b)+(c)+(d)"
  from "color edit → re-run (d) only".
  No structural changes needed beyond those flags. Phase 6 plan can be
  drafted once 2.x is in.

---

## 9. Summary of expected pixel-hash deltas

| Fixture                                                    | After 2.1 | After 2.2 | After 2.3 |
| ---------------------------------------------------------- | --------- | --------- | --------- |
| `mandelbrot/default_regression_test.json`                  | unchanged | unchanged | regen     |
| `mandelbrot/anti_aliasing_regression_test.json`            | unchanged | unchanged | regen     |
| `mandelbrot/downsample_interpolation_regression_test.json` | unchanged | regen     | regen     |
| `julia/default_regression_test.json`                       | unchanged | unchanged | regen     |
| `driven_damped_pendulum/default_regression_test.json`      | unchanged | unchanged | unchanged |
| `serpinsky/...`, `barnsley_fern/...`                       | unchanged | unchanged | unchanged |

`serpinsky` and `barnsley_fern` are out of `Renderable` scope per the parent
roadmap §9.5 and untouched by this work.
