# GUI Unification & Color-Sync Roadmap

This document is the canonical roadmap for the multi-phase plan to consolidate
the project onto a single cross-platform GUI architecture built on
`eframe`/`egui`, and to deliver a unified interactive experience that combines
fractal exploration with live color-map editing.

**Audience:** a new agent or contributor picking up the GUI work. This doc is
self-contained — no prior conversation context is needed.

**Scope:** everything from current state through to the end of "live color
edits visibly synced into the fractal preview." Out of scope: parameter
inspector panels, live fractal-type switching, support for fractal types not
already explorable today (BarnsleyFern, Serpinsky), undo/redo,
drag-and-drop on keyframes, save-back to the original input JSON.

---

## 1. End State Vision

The binary ships with exactly two modes:

1. **Headless render mode** (`fractal-renderer render <params.json>`) —
   unchanged. Writes images to disk based on a params JSON file. No GUI.
2. **Interactive mode** (`fractal-renderer explore <params.json>`) — a single
   unified GUI window that combines:
   - Fractal preview (pan/zoom/click).
   - Color-map editor (per-keyframe color + position, multi-gradient tabs for
     Newton).
   - Live preview updates as colors are edited.
   - Snapshot-to-disk via Space, capturing both the fully-rendered image and
     the synced parameter JSON (the saved JSON, when re-loaded, restores the
     GUI to exactly the captured state).

Built entirely on `eframe` (egui's official framework), with a background
render thread feeding a `TextureHandle` for live updates.

**What disappears over the course of this roadmap:**

- The legacy `pixels` crate and direct `winit` usage (already removed in
  Phases A+B; see §2).
- The `color_swatch` CLI subcommand and its supporting code.
- The standalone `color-gui-demo` example (folded into `explore`).
- The two separate eframe apps ([src/core/user_interface.rs](../src/core/user_interface.rs)
  and [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs)),
  absorbed into a unified `src/core/interactive/` module.

---

## 2. Current State (post Phases A+B)

Phases A (port explore mode to eframe; remove `pixels`) and B (Rust edition
2024 + eframe 0.34) have already landed on `main`. The current state:

### Dependencies

```toml
edition = "2024"
eframe = { version = "0.34", default-features = false, features = ["wgpu", "x11", "wayland"] }
egui = "0.34"
# pixels and direct winit have been removed entirely.
```

### Two independent eframe apps share no infrastructure

**Explore**

- **File:** [src/core/user_interface.rs](../src/core/user_interface.rs)
- **Status:** `eframe::App`; preview-only; full pan/zoom/save behavior

**Color editor**

- **File:** [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs)
- **Status:** `eframe::App`; demo widgets only — does not affect renderer

Both apps share [src/core/eframe_support.rs](../src/core/eframe_support.rs) for
`wgpu` setup, but everything else is duplicated.

### CLI

[src/cli/args.rs](../src/cli/args.rs) defines three subcommands: `Render`,
`Explore`, `ColorSwatch`. This roadmap focuses primarily on modifications to
`Explore`, while preserving functionality of `Render`. The `ColorSwatch` mode
will be removed. The `Explore` subcommand dispatches in
[src/cli/explore.rs](../src/cli/explore.rs) on the `FractalParams` variant:
Mandelbrot/Julia/DDP go through generic `PixelGrid<F>`; Newton has its own
explore path in [src/fractals/newtons_method.rs:461](../src/fractals/newtons_method.rs#L461).
BarnsleyFern and Serpinsky panic with "ERROR: Parameter type does not yet
implement RenderWindow" — they are intentionally out of scope for `explore`.

### Color-map representations (the central problem this roadmap addresses)

The three explorable fractal families today use structurally different color
representations:

**Mandelbrot, Julia**

- **Representation:** `ColorMapParams` = 1 gradient + 1 flat background ([src/fractals/quadratic_map.rs:19](../src/fractals/quadratic_map.rs#L19))

**Driven-damped pendulum**

- **Representation:** No color params at all — hard-coded white/black in `render_point` ([src/fractals/driven_damped_pendulum.rs:38-44](../src/fractals/driven_damped_pendulum.rs#L38-L44))

**Newton's method**

- **Representation:** `CommonParams` with `boundary_set_color_rgb` + `cyclic_attractor_color_rgb` + `ColorMapSpec` enum (FullColorSpec or GrayscaleSpec) ([src/fractals/newtons_method.rs:204-266](../src/fractals/newtons_method.rs#L204-L266))
  Unifying these is Phase 1.

---

## 3. Phase Roadmap Summary

| Phase | Title                                  | Blast radius                                                                              |
| ----- | -------------------------------------- | ----------------------------------------------------------------------------------------- |
| 1     | Color-map data unification             | All fractal params + every example/test JSON file                                         |
| 2     | Compute / color split                  | `Renderable` trait + new `RenderingPipeline` + JSON migration to `sampling_level`         |
| 3     | Pipeline unification & per-root colors | Lift AA logic into core; collapse `ColorMapKind` to one shape; per-root histograms / CDFs |
| 4     | Unified `FractalApp` shell             | New `src/core/interactive/` module; preview only                                          |
| 5     | Color editor panel                     | Editor widget + layout wiring                                                             |
| 6     | CLI + cleanup + Space-as-save          | Delete legacy modules; extend snapshot behavior                                           |
| 7     | Live color sync                        | `RenderingPipeline::recolorize_only` + dirty flags                                        |
| 8     | Polish                                 | Contents TBD post-Phase-7 measurement                                                     |

Phases 1, 2, and 3 are renderer-architecture pre-work; the GUI work proper
starts at Phase 4. Phases 4 → 7 are sequential. Phase 8 is opportunistic.

Each phase is a self-contained PR, bisectable, independently revertible.

### Status:

**Phase One**
Completed in 9a2e51b19a6baa7d119351d77902dba5c8aa171b.

**Phase Two**
Completed on branch `decouple-scalar-field-calculation-from-color-rendering`
in three commits: 4199c23 (machinery, parallel-to-old), 0caa21c (runtime
switch + JSON migration), 8008aff (full-field histogram).

**Phase Three**
Completed on branch `decouple-scalar-field-and-color-mapping-common-aa`
in two commits: 56ad860 (Phase 3.1 — `FieldKernel` + core iteration
helpers, parallel-to-old) and the follow-up that landed Phase 3.2 + 3.3
together: collapsed `ColorMapKind` to a unified `ColorMap`, dropped
`normalize_field` (CDF lookup now happens inside `colorize_cell`),
introduced per-root histograms / CDFs, dropped the `QuadraticMap<T>`
wrapper, mass-migrated every example/bench/test JSON via
`scripts/migrate_phase_3_color_maps.py`, restored the DDP regression
fixture, and added two Newton fixtures. Mandelbrot/Julia/Barnsley/
Sierpinski hashes invariant; DDP visually identical (constant-color
gradient) but bit-shift due to cell-type change; Newton hashes new
(per-root contrast improvement).

---

## 4. Hard Constraints & Cross-Platform Learnings

These are preserved from cross-platform work during Phases A+B. They remain
relevant to any GUI work going forward.

### 4.1 Border / line artifacts at panel boundaries

**Symptom:** thin white or gray lines appear between `SidePanel` and
`CentralPanel` when the window is maximized or fullscreen, especially at
fractional DPI.

**Root causes (multiple):**

1. Default `SidePanel` draws a 1px separator line via
   `visuals.widgets.noninteractive.bg_stroke` (gray(60) in dark theme).
   → Fix: `show_separator_line(false)` + set `bg_stroke = Stroke::NONE`.
2. Default `panel_fill = gray(27)` against black background shows 1px gaps at
   panel seams due to `rect.shrink(1.0)` in egui's panel drawing code.
   → Fix: set `panel_fill = Color32::BLACK` and override each panel with
   `Frame::none().fill(Color32::BLACK)`.
3. Manual 1-logical-pixel strokes at fractional x-positions anti-alias across
   two physical pixels (e.g. `paint_gradient_bar` using `line_segment`).
   → Fix: use `painter.rect_filled` with contiguous rectangles instead.

Both existing apps already apply these fixes; the unified app must too.

### 4.2 Resize event drops on WSL/XWayland

**Symptom:** window appears not to resize, or content stops updating when the
user drags the window edge.

**Mitigation:** call `ctx.request_repaint_after(IDLE_TICK)` at the end of
every `update()` so eframe re-polls surface size every ~100ms. Already
implemented in both apps (see [src/core/user_interface.rs:259](../src/core/user_interface.rs#L259)).

### 4.3 egui panel width locking

`SidePanel::exact_width(w)` clamps `width_range` to `w..=w`, making the panel
non-resizable even though the resize drag handle still renders. **Use
`default_width(w).width_range(min..=max)` instead.**

### 4.4 Adaptive device limits

`wgpu_core` rejects requests for limits the GPU doesn't expose
(`max_color_attachments`, etc.). Virtualized and software drivers
(WSL/XWayland, llvmpipe) routinely expose only 2-4. Solution lives in
[src/core/eframe_support.rs](../src/core/eframe_support.rs): clone the
adapter's own limits into the device descriptor. Both existing apps go through
this helper; the unified app must too.

### 4.5 Wgpu version coupling (no longer load-bearing, but worth knowing)

`wgpu_core` exports `#[no_mangle]` C symbols. Two versions in the same binary
→ linker error. This is why `pixels` had to be removed before `eframe` could
be upgraded. The dep tree is now clean (`eframe 0.34` only); future
upgrades within the eframe family are unconstrained.

---

## 5. Data Model

> **Phase-2 vs Phase-3 note.** Phase 2 shipped with three concrete
> `ColorMapKind` variants (`ForegroundBackground`, `BackgroundWithColorMap`,
> `MultiColorMap`), three different `Cell` shapes, and a four-phase pipeline
> with a `normalize_field` step that CDF-rewrote the field in place. Phase 3
> collapses all of that. The data model below describes the **post-Phase-3**
> end state. The earlier shape is preserved in commits 4199c23 / 0caa21c /
> 8008aff for reference.

A single uniform color-map type, a single uniform cell shape, and a
five-phase pipeline serve every fractal family. Per-fractal customization
reduces to one method (`evaluate(point) -> Cell`) plus static config.

The field shape is `Vec<Vec<Cell>>` with `Cell = Option<(f32, u32)>` — the
`f32` is the raw scalar value (smooth iteration count, basin marker, etc.)
and the `u32` is the _gradient index_ picking which gradient to colorize
through. Mandelbrot/Julia/DDP always emit gradient index 0; Newton emits the
root index. The field stays raw end-to-end — there is no `normalize_field`
pass; CDF percentile lookup happens inside `colorize_cell`.

### 5.1 The unified `ColorMap` type

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorMap {
    /// Color used for cells whose evaluation produced no scalar (Mandelbrot
    /// in-set, DDP out-of-basin, Newton non-converging).
    pub flat_color: [u8; 3],
    /// One gradient per "channel". Mandelbrot/Julia/DDP have `len() == 1`;
    /// Newton has one entry per root. The `u32` in each cell indexes into
    /// this vec.
    pub gradients: Vec<Vec<ColorMapKeyFrame>>,
}
```

DDP's degenerate "all foreground" case is encoded as a single-keyframe
gradient (a constant-color gradient — the foreground color repeated at
`query=0.0` and `query=1.0`).

The `ColorMapKind` trait collapses to a single impl on `ColorMap`. With one
impl the trait carries no weight, so it is dropped — `ColorMap` exposes its
`create_cache` / `refresh_cache` / `colorize_cell` as inherent methods, and
the pipeline operates over a concrete `ColorMap` rather than a generic
parameter.

```rust
impl ColorMap {
    /// Allocate the cache once at pipeline construction.
    /// `lookup_table_count` sets the resolution of each gradient's LUT.
    pub fn create_cache(&self, lookup_table_count: usize) -> ColorMapCache;

    /// Rebuild the cache in place from current keyframes / flat colors.
    /// Allocation-free. Re-runs after keyframe edits (Phase 7).
    pub fn refresh_cache(&self, cache: &mut ColorMapCache);
}

pub struct ColorMapCache {
    /// Per-gradient CDF. Length matches `ColorMap::gradients.len()`.
    /// Refreshed by the pipeline after each compute pass.
    pub cdfs: Vec<CumulativeDistributionFunction>,
    /// Per-gradient LUT, `[0,1]`-domain. Refreshed by `refresh_cache`.
    pub luts: Vec<ColorMapLookUpTable>,
    /// Pre-converted `flat_color`.
    pub flat: Color32,
}

/// Per-cell colorize. Statically dispatched, called inside the AA-collapse
/// loop. CDF lookup + LUT lookup happen here, in color space.
#[inline]
pub fn colorize_cell(cache: &ColorMapCache, cell: Option<(f32, u32)>) -> [u8; 3] {
    match cell {
        Some((value, gradient_index)) => {
            let g = gradient_index as usize;
            let pct = cache.cdfs[g].percentile(value);
            cache.luts[g].lookup(pct)
        }
        None => [cache.flat.r(), cache.flat.g(), cache.flat.b()],
    }
}
```

### 5.2 The `Renderable` / `FieldKernel` split

`FieldKernel` is the small surface every fractal must implement —
domain-specific scalar evaluation at one point. `Renderable` extends it with
housekeeping (params, image spec, diagnostics). All AA / block-fill
iteration logic lives in core helpers generic over `K: FieldKernel`; no
fractal duplicates the parallel-iter skeleton.

```rust
/// Domain-specific per-point evaluation. Each fractal implements exactly
/// this much of the math.
pub trait FieldKernel: Sync + Send {
    /// Evaluate the scalar field at one real-space point.
    /// Returns `Some((value, gradient_index))` or `None` for "no value".
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)>;
}

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

The hot path is generic over `F: Renderable`. Dispatch happens once, at the
top of [src/cli/explore.rs](../src/cli/explore.rs) where `match
fractal_params { … }` selects the concrete `F` to instantiate. From there
inward every call site is monomorphized.

### 5.3 The five-phase `RenderingPipeline`

A single top-level orchestrator, parameterized by `F: Renderable`, owns all
reusable buffers. The pipeline is broken into five steps, but only step (a)
is fractal-specific — the rest is shared core code:

```rust
pub struct RenderingPipeline<F: Renderable> {
    fractal: F,
    field: Vec<Vec<Option<(f32, u32)>>>,
    /// One histogram per gradient. Length matches `fractal.color_map().gradients.len()`.
    histograms: Vec<Histogram>,
    color_cache: ColorMapCache,
    n_max_plus_1: usize,
}

impl<F: Renderable> RenderingPipeline<F> {
    pub fn render(&mut self, out: &mut egui::ColorImage, sampling_level: i32) {
        // (a) Fill the field with raw values via the fractal's FieldKernel.
        core::field_iteration::compute_raw_field(
            self.fractal.image_specification(),
            self.n_max_plus_1, sampling_level, &self.fractal, &mut self.field);

        // (b) Bin into per-gradient histograms.
        for h in &mut self.histograms { h.reset(); }
        core::field_iteration::populate_histograms(
            self.n_max_plus_1, sampling_level, &self.field, &mut self.histograms);

        // (c) Rebuild per-gradient CDFs.
        for (cdf, hist) in self.color_cache.cdfs.iter_mut().zip(&self.histograms) {
            cdf.reset(hist);
        }

        // (d) Refresh LUTs from current keyframes.
        self.fractal.color_map().refresh_cache(&mut self.color_cache);

        // (e) Walk field; CDF + LUT lookup per cell; AA-average per output pixel.
        core::field_iteration::colorize_collapse(
            &self.color_cache, &self.field,
            self.n_max_plus_1, sampling_level, out);
    }
}
```

There is no `normalize_field` pass and no per-cell `apply_cdf`. The field
stays raw; the CDF lookup happens inside `colorize_cell` at colorize time.
This means:

- **Keyframe edits** invalidate only the LUTs (and optionally the
  `flat_color`). Re-run (d) + (e); skip (a)/(b)/(c). Phase 7 lives here.
- **No race between normalize and colorize**: the field is only ever
  written by (a) and only ever read by (b)/(e).
- **Per-root histograms come for free**: Newton naturally bins into
  separate histograms per root, so each basin gets its own CDF over its own
  iteration-count distribution. The other fractals trivially reduce to a
  single histogram.

`colorize_collapse` is a generic free function (not a trait method): it
walks the row-major output `egui::ColorImage` and per output pixel reads
the corresponding `(n+1)²` block of the field, calls `colorize_cell` per
subpixel, and averages. Per-pixel hot path: zero allocation, zero `dyn`.

### 5.4 Allocation strategy

All buffers are allocated **once per session** (or per window resize) on
`RenderingPipeline::new` / `resize`, never per frame:

- `field` is sized for `(n_max+1)·W × (n_max+1)·H` where `n_max+1` derives
  from the user's JSON `sampling_level` (positive AA values cap field
  size). Reallocated only on window resize.
- `histograms` (one per gradient) and `color_cache` (containing per-gradient
  CDFs and LUTs) are independent of resolution; each is allocated once and
  reset/refreshed in place. Per-gradient vec lengths are determined at
  construction from `fractal.color_map().gradients.len()`.
- The output `egui::ColorImage` is owned by `PixelGrid`, sized to `[W, H]`,
  reallocated only on resize.

Per-frame allocations: zero. Per-(sub)pixel allocations: zero.

### 5.5 The `sampling_level` model

`RenderOptions` collapses today's `subpixel_antialiasing: u32` and
`downsample_stride: usize` into a single `sampling_level: i32`:

| `sampling_level` | Field cells per output pixel | Output pixels per field cell | Mode                    |
| ---------------- | ---------------------------- | ---------------------------- | ----------------------- |
| `+n` (n > 0)     | `(n+1)²`                     | 1                            | Anti-alias              |
| `0`              | 1                            | 1                            | Baseline                |
| `−n` (n > 0)     | sparse                       | `(n+1)²`                     | Block-fill (downsample) |

The JSON-supplied `sampling_level` is the **maximum** the pipeline ever uses
(the cap that determines field buffer size). The
`AdaptiveOptimizationRegulator` drives the **runtime** value passed to
`RenderingPipeline::render`: at full quality it equals the user value; under
interactive load it drops toward 0 and into the negative range as needed.

Block-fill is nearest-neighbor / zero-order hold — today's bilinear
interpolation between sparse samples is dropped in Phase 2.2. Users see
`(n+1)²`-pixel blocks while interactive; full-resolution returns when
quality climbs back to baseline.

### 5.6 Why a single uniform color-map type

A single `ColorMap` shape with `Vec<gradient>` keeps:

- **All AA / block-fill iteration in core.** Three core helpers
  (`compute_raw_field`, `populate_histograms`, `colorize_collapse`) each
  consume the same `Vec<Vec<Option<(f32, u32)>>>` field and the same
  `&ColorMapCache`. No fractal code touches the parallel-iter skeleton.
- **The colorize hot path allocation-free.** The cache is reused in place;
  no per-render `Vec` construction.
- **One LUT shape, one CDF shape, one cell shape.** Mandelbrot/Julia/DDP
  reduce to `gradients.len() == 1`; Newton uses N>1. There is no
  "single-gradient fast path" — N=1 is the same path as N=many, just with
  a unit-length vec.
- **The editor static** (Phase 5): the editor widget operates on a single
  concrete `ColorMap` type. Per-fractal customization lives in the
  per-fractal renderer, not the editor.

The earlier (pre-Phase-3) shape had three separate `ColorMapKind` impls,
three different `Cell` types, and a `normalize_field` pass that CDF-rewrote
the field. That shape carried duplicated AA-iteration logic into every
fractal because the cell type varied per fractal. Phase 3 unifies the cell
type, which lets the AA iteration sit in one place — generic over the
concrete fractal's `FieldKernel::evaluate`, not over a varying `Cell` type.

---

## 6. Phase Detail

### Phase 1 — Color-map data unification

**Goal:** introduce the per-variant concrete color-map structs and embed each
fractal type's matching struct directly in its params. JSON schema migrates
accordingly. No GUI work, no trait changes (Phase 2 handles trait wiring).

**Files touched:**

- [src/core/color_map.rs](../src/core/color_map.rs) — define
  `ForegroundBackground`, `BackgroundWithColorMap`, `MultiColorMap` concrete
  structs (per §5.1). Optionally define the `UnifiedColorMap` boundary-only
  enum and `From` impls (per §5.4); skip if no caller materializes for them
  in this phase.
- [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs) — replace
  `ColorMapParams.keyframes` and `background_color_rgb` with `pub color: BackgroundWithColorMap`.
  The other `ColorMapParams` fields
  (`lookup_table_count`, `histogram_bin_count`, `histogram_sample_count`) are
  not color data and stay on `QuadraticMapParams` directly (or move into a
  sibling struct — implementer's choice).
- [src/fractals/mandelbrot.rs](../src/fractals/mandelbrot.rs),
  [src/fractals/julia.rs](../src/fractals/julia.rs) — automatic via the
  `QuadraticMapParams` trait change.
- [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs)
  — add `pub color: ForegroundBackground` field with
  `#[serde(default = "ddp_default_color")]`. Default is
  `ForegroundBackground { foreground: [255,255,255], background: [0,0,0] }`,
  matching the previously hard-coded values. Replace the literal
  `Rgb([255,255,255])` / `Rgb([0,0,0])` in `render_point` with reads from the
  field.
- [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs) —
  replace `boundary_set_color_rgb`, `cyclic_attractor_color_rgb`, and
  `color_map_spec` in `CommonParams` with a single `pub color: MultiColorMap`
  field. **Drop `GrayscaleSpec` and `GrayscaleKeyframeSpec` entirely**,
  including the `to_color_map_vec` expansion logic. The `ColorMapSpec` enum
  is removed (its `FullColorSpec` variant becomes redundant once
  `MultiColorMap` is the embedded type).
- Every JSON file under [examples/](../examples/), [benches/](../benches/),
  and [tests/param_files/](../tests/param_files/) that references a fractal
  whose schema changed. The
  [examples/explore-newton-roots-of-unity-4/params.json](../examples/explore-newton-roots-of-unity-4/params.json)
  file (and its render counterpart) currently use `GrayscaleSpec`; expand by
  hand into 4 explicit per-root gradients.
- [tests/example_parameter_validation_tests.rs](../tests/example_parameter_validation_tests.rs)
  — verify still passes against migrated JSONs.

**Verification:** `cargo test` — pixel-hash regression tests in
[tests/full_cli_integration_and_regression_tests.rs](../tests/full_cli_integration_and_regression_tests.rs)
must remain unchanged (color computation is logically identical, only the
schema moved).

**Static-typing invariant:** each fractal embeds its concrete color-map
struct directly. There is no enum wrapper at the params level and no runtime
"is this the right variant" check. Wrong-shape JSON fails serde
deserialization with a structured error before any fractal object is
constructed; once construction succeeds, the type is permanently fixed.

### Phase 2 — Compute / color split

**Goal:** factor `Renderable` so per-pixel scalar computation is separated
from colorization. Hoist histogram, CDF, and lookup-table state into a
top-level `RenderingPipeline<F>` per §5.3. Replace the two-axis
`(subpixel_antialiasing, downsample_stride)` knob with a unified signed
`sampling_level: i32`. Land an intentional pixel-hash bump for
Mandelbrot/Julia/Newton when the histogram switches from a sub-sample grid
to the full field — DDP unaffected.

**Status: shipped.** See commits 4199c23 / 0caa21c / 8008aff on branch
`decouple-scalar-field-calculation-from-color-rendering`. The detailed plan
file (`phase-2-detailed-plan.md`) was deleted post-merge.

**Architectural deviations from the original spec, preserved here for
reference:**

- The "bit-equivalent to the old path" claim for 2.1 was impossible given
  the `[0,1]`-domain LUT chosen for Phase-7 cache-validity reasons. The 2.1
  equivalence test ran with a tolerance of `MAX_GRADIENT_DIFF = 32`
  per-channel for gradient fractals; DDP stayed strict.
- All Mandelbrot/Julia pixel hashes regenerated in 2.2 (not just the
  downsample fixture as predicted). DDP/Barnsley/Sierpinski unchanged.

These deviations are now moot because Phase 3 replaces the `ColorMapKind`
trait + per-cell `apply_cdf` shape entirely.

### Phase 3 — Pipeline unification & per-root colors

**Status: shipped.** Two commits on branch
`decouple-scalar-field-and-color-mapping-common-aa`:
56ad860 (3.1 machinery, parallel-to-old) and the follow-up that
collapsed the trait surface, dropped `normalize_field`, switched to
per-root histograms / CDFs, and migrated every fixture JSON.

**Goal:** finish what Phase 2 started. Lift all AA / block-fill iteration
out of per-fractal code and into core. Collapse the three `ColorMapKind`
variants into one `ColorMap` shape. Drop the `normalize_field` pipeline
phase entirely; CDF lookup happens at colorize time. Switch Newton to
per-root histograms (one CDF per root over its own iteration-count
distribution).

**Detailed plan:** see [phase-3-detailed-plan.md](phase-3-detailed-plan.md)
for trait shapes, file lists, commit-by-commit verification, and the JSON
migration script.

**Implementation deviations from the planned commit boundary:** the
plan sketched three commits (3.1/3.2/3.3) but called the boundaries
"final breakdown TBD". The actual ship was two commits because the
3.2/3.3 work was deeply intertwined: dropping the per-fractal iteration
methods (3.2) requires the field cell type to be uniform, which is
exactly what the `ColorMapKind` collapse (3.3) provides. Splitting them
would have required either a temporary lossy bridging layer (per-fractal
`Option<(f32, u32)>` → variant-specific cell type) or duplicating the
trait surface for one commit. Either option was uglier than landing the
two together. The `QuadraticMap<T>` wrapper was also dropped here (open
question §13.1) — Mandelbrot and Julia now implement `Renderable` /
`FieldKernel` / `SpeedOptimizer` via blanket impls over
`T: QuadraticMapParams`.

**Why now (vs. shipping with Phase 2):** the Phase 2 review surfaced that
the AA / block-fill iteration logic was duplicated across all three
fractals — every fractal's `compute_raw_field` / `populate_histogram` /
`normalize_field` had the same `if sampling_level >= 0 / else` skeleton, the
same `outer_x % n_max_plus_1` index arithmetic, and the same parallel-iter
boilerplate. The fix needed to land before the GUI work in §4-§7 because
the GUI editor (Phase 5+) depends on the unified `ColorMap` shape.

**Pixel-hash impact:**

- Mandelbrot/Julia: invariant. With one root, per-root histogram = today's
  single histogram; moving CDF lookup from normalize-pass to colorize-time
  is mathematically a no-op.
- DDP: hashes regenerated once. DDP is now histogrammed (it was a no-op
  before); the resulting image is identical because the gradient is
  constant-color, but the bit-level encoding shifts because the field type
  changes from `Option<i32>` to `Option<(f32, u32)>`.
- Newton: hashes regenerated once. Per-root CDFs are a real algorithmic
  improvement — each basin gets its own iteration-count distribution.

**Sketched commit structure (final breakdown TBD):**

- **3.1 — Add `FieldKernel` trait + core iteration helpers, parallel-to-old.**
  New `src/core/field_iteration.rs` with `compute_raw_field`,
  `populate_histograms`, and the existing `colorize_collapse` (moved from
  `render_pipeline.rs`). Unit-test all three with synthetic kernels. Old
  per-fractal `compute_raw_field` etc. retained.
- **3.2 — Migrate fractals to `FieldKernel`; delete per-fractal duplicates.**
  Each fractal implements only `evaluate(point) -> Option<(f32, u32)>`.
  Pixel hashes invariant for Mandelbrot/Julia (still using current
  `ColorMapKind` machinery).
- **3.3 — Collapse `ColorMapKind` variants to one `ColorMap` type.**
  Drop `ForegroundBackground`, `BackgroundWithColorMap`, `MultiColorMap` as
  separate types. Drop `normalize_field` from the pipeline; move CDF lookup
  into `colorize_cell`. Per-root histograms for Newton. Mass JSON migration
  via `scripts/migrate_phase_3_color_maps.py`. Regenerate DDP and Newton
  pixel hashes (Mandelbrot/Julia stay invariant).

### Phase 4 — Unified `FractalApp` shell

**Goal:** introduce a new `src/core/interactive/` module hosting a single
`eframe::App` that handles all four explorable fractal types. Preview only;
no color editor yet. `Cargo run -- explore <params.json>` continues to work
across Mandelbrot, Julia, DDP, and Newton — same pan/zoom/click/save
behavior as today's explore mode.

**Files touched:**

- [src/core/mod.rs](../src/core/mod.rs) — add `pub mod interactive;`.
- `src/core/interactive/mod.rs` — new; re-exports public API.
- `src/core/interactive/app.rs` — new; the `FractalApp<F: Renderable>` struct
  and its `eframe::App` impl. Lifted from
  [src/core/user_interface.rs](../src/core/user_interface.rs) with no
  behavior changes.
- [src/cli/explore.rs](../src/cli/explore.rs) — dispatch on `FractalParams`
  variant, calling `interactive::run::<F>(...)` instead of
  `user_interface::explore::<F>(...)`. Newton's separate
  [src/fractals/newtons_method.rs:461](../src/fractals/newtons_method.rs#L461)
  `explore_fractal` similarly retargets.

The old [src/core/user_interface.rs](../src/core/user_interface.rs) stays
in tree at this phase to keep diffs reviewable; it gets deleted in Phase 6.

**Visuals:** `panel_fill = BLACK`, `bg_stroke = NONE`, `Frame::NONE.fill(BLACK)`
on every panel — matches current explore mode and avoids border artifacts
(§4.1).

**Verification:** manual smoke-test all four fractal types on various platforms.

### Phase 5 — Color editor panel

**Goal:** add the right-side color editor panel to `FractalApp`. Editor
displays the loaded color map and allows local mutation of a **cached
copy**. The fractal preview is not affected by edits — that's Phase 7.

**Files touched:**

- `src/core/interactive/editor.rs` — new. Defines a `show_editor` function
  on the unified `ColorMap` type:
  ```rust
  impl ColorMap {
      pub fn show_editor(&mut self, ui: &mut egui::Ui, state: &mut EditorState) -> bool;
  }
  ```
  When `gradients.len() == 1` the tab strip is suppressed; otherwise a
  per-gradient tab strip selects which gradient the keyframe widgets edit.
  Shared widget helpers (`show_swatch`, `show_gradient_segment`, fraction
  renormalization) live as free functions in this module.
- `src/core/interactive/app.rs` — extend layout: `SidePanel::right` for
  the editor, `CentralPanel` for the preview. `FractalApp<F>` gains
  `editor_color_map: ColorMap` (a clone of the renderer's color map) and a
  small `EditorState` for selection (selected keyframe index, active
  gradient tab). Calls
  `self.editor_color_map.show_editor(ui, &mut self.editor_state)` in the
  panel.

**Layout:**

```
┌─────────────────────────────────────────────┬──────────────────────┐
│                                             │ Color Map            │
│                                             │ ───────────────────  │
│                                             │ [Newton tabs only:]  │
│                                             │ │Root 0│Root 1│...│  │
│                                             │ ───────────────────  │
│         (fractal preview, central)          │ Flat colors:         │
│                                             │  [swatch] background │
│                                             │ ───────────────────  │
│                                             │ Keyframes:           │
│                                             │  [color cell #0]     │
│                                             │  [+] [0.25]          │
│                                             │  [color cell #1]     │
│                                             │  [+] [0.30]          │
│                                             │  ...                 │
│                                             │ ───────────────────  │
│                                             │ [gradient bar]       │
│                                             │ ───────────────────  │
│                                             │                      │
│                                             │ [color picker]       │
│                                             │                      │
└─────────────────────────────────────────────┴──────────────────────┘
```

Detailed widget spec is in §7.

**Local-cache lifecycle:** `editor_color_map: ColorMap` is initialized at
startup as a clone of `renderer.color_map()`. All editor widgets mutate only
this cache. The renderer continues to use its own (immutable, in this phase)
color map. Edits do not survive window close (no save-back to disk;
Space-as-save in Phase 6 captures the cache to a fresh timestamped JSON).

### Phase 6 — CLI + cleanup + extended Space-as-save

**Goal:** retire dead code paths; extend snapshot behavior to capture color
edits.

**Files touched (deletions):**

- [src/cli/color_swatch.rs](../src/cli/color_swatch.rs) — delete entirely.
- [src/cli/args.rs](../src/cli/args.rs) — remove `ColorSwatch` variant from
  `CommandsEnum`.
- [src/cli/mod.rs](../src/cli/mod.rs) — remove `pub mod color_swatch;`.
- [src/main.rs](../src/main.rs) — remove `ColorSwatch` dispatch arm and the
  `use cli::color_swatch::generate_color_swatch` import.
- [examples/visualize-color-swatch-rainbow/](../examples/visualize-color-swatch-rainbow/) — delete.
- [examples/color-gui-demo/](../examples/color-gui-demo/) — delete (its
  functionality is now part of `explore`).
- [examples/common/mod.rs](../examples/common/mod.rs) — delete
  `color_swatch_example_from_string` and `color_editor_example_from_string`.
- [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs) —
  delete; absorbed into `src/core/interactive/editor.rs`.
- [src/core/user_interface.rs](../src/core/user_interface.rs) — delete;
  absorbed into `src/core/interactive/app.rs`.
- [src/core/mod.rs](../src/core/mod.rs) — remove deleted module decls.

**Files touched (extension):**

- `src/core/interactive/app.rs` — implement the new Space-as-save behavior
  (full spec in §8). At this phase, "sync color map back into renderer"
  is a one-shot copy from `editor_color_map` into the renderer's params
  before the snapshot render — Phase 7 turns this into a continuous flow.
- [src/core/render_window.rs](../src/core/render_window.rs) — `PixelGrid`
  may need an explicit "render at full quality, then notify" entry point to
  support the save flow (the existing `render_to_file` path can be retained
  or rewired; implementer's choice).

**Verification:** all CI green. Manual smoke-test of Space-as-save: load
example, edit colors, press Space, verify (a) overlay appears, (b) controls
locked during render, (c) timestamped JSON + PNG written to disk, (d)
re-loading the saved JSON via `cargo run -- explore <saved.json>` reproduces
the exact GUI state including colors.

### Phase 7 — Live color sync

**Goal:** color edits in the editor panel cause the fractal preview to
re-colorize live (target: <1 frame latency at 1080p).

**Approach:** `RenderingPipeline` owns the raw field buffer plus the
per-gradient CDFs that persist across renders. Phase 7 adds two dirty flags
so the pipeline can run only steps (d)+(e) when keyframes change, and the
full (a)→(e) sequence when the viewport changes. Because the field stays
raw and the CDFs are computed from the field (not the keyframes), keyframe
edits don't invalidate the CDFs — the recolorize fast path is exactly
"refresh LUTs + walk field again."

**Files touched:**

- [src/core/image_utils.rs](../src/core/image_utils.rs) — add
  `color_map_mut(&mut self) -> &mut ColorMap` to `Renderable`.
- [src/core/render_pipeline.rs](../src/core/render_pipeline.rs):
  - Add a `recolorize_only(&mut self, out, sampling_level)` method that
    runs `refresh_cache` + `colorize_collapse` against the existing field
    (skipping (a)/(b)/(c)).
- [src/core/render_window.rs](../src/core/render_window.rs) — extend
  `PixelGrid<F>` to:
  - Add `Arc<AtomicBool>` `field_dirty` (set on viewport change, triggers
    full pipeline) and `Arc<AtomicBool>` `color_dirty` (set on keyframe
    edit, triggers `recolorize_only`).
  - In `update()`, dispatch the appropriate background task based on which
    flag is set, with `field_dirty` taking priority if both are set.
- `src/core/interactive/app.rs` — wire editor edits: when the editor
  mutates a keyframe / fraction / flat color, write the change into
  `renderer.color_map_mut()`, set `color_dirty`, call `ctx.request_repaint()`.
- `src/core/interactive/editor.rs` — `show_editor` already returns whether
  anything changed (per Phase 5); the app uses that boolean to gate
  `color_dirty`.

**Editor cache transition:** the separate `editor_color_map: ColorMap` from
Phase 5 becomes redundant. The editor now mutates `renderer.color_map_mut()`
directly. The app retains only editor _selection_ state (selected keyframe
index, active gradient tab) — the data lives on the renderer.

**AA on re-colorize.** The cached field in `RenderingPipeline` is at
whatever upsampling factor the most recent (a)→(c) pass produced. Color
edits replay (d)+(e) at the same upsampling factor, so AA-quality
re-colorize during full-quality runs comes for free. During interactive
sampling (stride > 1, sparse field), re-colorize naturally honors the same
stride.

**Adaptive quality regulator interaction:** color edits trigger only
re-colorize, not re-compute, so the regulator's compute-quality scaling is not
mechanically engaged by them. Whether color edits should also feed the
`user_interaction = true` signal (so the regulator stays in "interactive
mode" and defers expensive idle-time recomputes) is a UX-feel decision to
make once Phase 7 is functional. The regulator self-tunes from observed
compute time, so neither choice is structurally wrong.

**Verification:** manual interactive testing — drag fraction sliders, click
keyframe colors, verify the preview updates within a frame or two. Benchmark
`colorize_collapse` over a representative populated field at 1920×1080 to
confirm it stays under one frame at 24Hz; if not, Phase 8 must include
tweaks to the adaptive quality scaling to make the UI feel smooth.

### Phase 8 — Polish

Contents to be defined post-Phase-7 measurement. Likely candidates:

- Debouncing rapid slider drags if `colorize` proves expensive at large
  resolutions.
- Tuning the defensive `request_repaint_after` cadence.
- Visual feedback for the selected keyframe (border, highlight).
- Color picker UX refinements (RGB vs HSV, eyedropper, swatch history).
- Whether to feed color edits into the adaptive regulator's
  `user_interaction` signal.

---

## 7. Color Editor Widget Spec

### 7.1 Single-gradient editor (used by each gradient tab)

**Read-only displays:**

- Vertical sequence of color cells, one per keyframe. Each cell is a small
  filled rectangle (~32×32px) showing the keyframe's RGB. Selectable.
- A horizontal gradient bar showing the full gradient as currently
  configured. Read-only — no drag-to-edit, no click-to-insert.

**Mutable handles:**

- Between each pair of adjacent keyframes: a `+` button and a `DragValue`
  showing the _fraction_ of the gradient occupied by that segment (the
  difference between the two adjacent keyframe positions).
- Inline color picker (egui's `color_picker_color32`), permanently visible
  at the bottom of the panel.

**Interactions:**

- **Click a color cell** → that keyframe becomes the selected keyframe.
  The inline color picker switches to editing its color. Live: every picker
  change writes into the keyframe's `rgb_raw`.
- **`Delete` key** while a keyframe is selected → that keyframe is removed
  from the gradient; selection clears; the picker returns to its idle state.
  The first and last keyframes (positions 0.0 and 1.0) are anchors and
  cannot be deleted (`Delete` is a no-op on them).
- **`Escape` key** → clears keyframe selection. Picker returns to its idle
  state. **`Escape` does not exit the application.**
- **`+` button** between two adjacent keyframes → inserts a new keyframe at
  the midpoint of that segment. Default color: linearly interpolated between
  the two adjacent keyframes (so insertion is initially invisible until the
  user edits the new keyframe). The `+` button does not appear before the
  first keyframe or after the last.
- **Edit a fraction `DragValue`** → that fraction adopts the new value; the
  _other_ fractions are scaled proportionally so the sum stays 1.0; the
  keyframe positions are recomputed from the resulting fractions. Each
  fraction is clamped to `[ε, 1.0]` (with `ε ≈ 0.001`) to prevent any
  segment from collapsing to zero width.

### 7.2 Layout

The unified `ColorMap` always renders the same widget shape:

- One color picker row at top labeled per-fractal (`Background` for
  Mandelbrot/Julia/DDP; `Cyclic attractor` for Newton) that edits
  `flat_color`.
- A tab strip (one tab per entry in `gradients`). When `gradients.len() == 1`
  the tab strip is suppressed and the lone gradient's editor renders directly.
  Otherwise tabs are labeled "Root 0", "Root 1", … and the active tab shows
  the single-gradient editor for that gradient. Switching tabs resets keyframe
  selection.

### 7.3 Application keys (interactive mode)

| Key                 | Behavior                                                                         |
| ------------------- | -------------------------------------------------------------------------------- |
| Arrow keys          | Pan view (existing).                                                             |
| W / S               | Zoom in / out (existing).                                                        |
| A / D (with no W/S) | Fast zoom in / out (existing).                                                   |
| R                   | Reset to initial view (existing) and color map (new)                             |
| Mouse left-click    | Recenter view on clicked point in the fractal preview (existing).                |
| Space               | Save snapshot — see §8.                                                          |
| Q                   | Exit application.                                                                |
| Ctrl+C              | Exit application (terminal default).                                             |
| Esc                 | Clear keyframe selection. **No-op when no keyframe is selected.** Does not exit. |
| Delete              | Remove selected keyframe (no-op for first/last).                                 |

The Esc-as-quit binding present in today's [src/core/user_interface.rs:216](../src/core/user_interface.rs#L216)
must be removed.

### 7.4 Out of scope

- Drag-and-drop on the gradient bar or color cells.
- Arrow-key / Tab navigation between keyframes (click only).
- Undo / redo.
- Adding or removing entire gradients (e.g. changing Newton's root count).
- Reordering keyframes (positions are derived from fractions, which are
  positive, so order is stable).

---

## 8. Space-as-Save Spec

Pressing Space initiates a deliberate "publish this exact state" action.
Unlike today's fire-and-forget snapshot, the new flow is gated on a
full-quality render and locks input (with user feedback) until complete.

### 8.1 State machine

```
Idle ──Space pressed──► Saving ──save complete──► Idle
                          │
                          ├── overlay shown
                          ├── input locked
                          ├── adaptive regulator forced to level 0
                          └── re-render in flight
```

### 8.2 Step sequence

1. **Lock & overlay.** Set `save_in_progress = true`. Display a feedback
   overlay (translucent panel, "Saving snapshot…"). All input is suppressed
   for the duration: pan/zoom keys, click-to-center, Space (debouncing
   double-press), Esc, color edits.
2. **Force quality to default.** Reset
   `AdaptiveOptimizationRegulator` so the next render uses
   `speed_optimization_level = 0.0`. The field will be computed at full
   user-specified quality, not whatever degraded state interactive use had
   pushed it to. Consider caching the current value of the quality so that it
   can immediately be restored on the next user interaction to enable quick response.
3. **Sync color map.** Push the editor's current color map (which in Phase 7
   _is_ `renderer.color_map_mut()`; in Phase 6 was a separate
   `editor_color_map: ColorMap` cache) into the renderer's params for
   serialization.
4. **Render to GUI.** Background thread runs `compute_field` (full quality)
   followed by `color_map().colorize_into(...)` and swaps the result into
   the preview texture. The save flow blocks (overlay still up) until the
   render is complete.
5. **Save params to disk.** Serialize the now-synced `FractalParams`
   (including the embedded concrete color-map struct and the current
   view-control's `image_specification`) to `<prefix>_<datetime>.json`. The
   filename pattern matches today's
   [src/core/render_window.rs:255-261](../src/core/render_window.rs#L255-L261).
6. **Save image to disk.** Write the just-rendered buffer to
   `<prefix>_<datetime>.png`. Pixels match what's on screen.
7. **Unlock.** Clear `save_in_progress`; remove overlay; resume input.

### 8.3 Restorability invariant

Calling `cargo run -- explore <saved.json>` on the file produced by step 5
must restore the GUI to _exactly_ the state it was in when Space was pressed:
the same view bounds, the same color map (including any edits), the same
render quality parameters, the same fractal type. The pixel hash of the rendered preview
should match the saved PNG.

### 8.4 Comparison to current behavior

Today's Space ([src/core/render_window.rs:254-280](../src/core/render_window.rs#L254-L280))
snapshots whatever the display buffer happens to contain, allowing the user
to keep interacting during the write. The new flow is the opposite: deliberate
"publish this exact state" gated on a full-quality render. The user accepts
a brief block in exchange for guaranteed fidelity between what's on screen,
what's written to disk, and what re-loads next time.

---

## 9. Threading & Adaptive Quality

### 9.1 Thread layout

- **UI thread:** eframe app — layout, input, editor mutations.
- **Background thread:** `PixelGrid` worker — runs `RenderingPipeline::render`
  and `recolorize_only`. The existing `Arc<Mutex<RenderingPipeline<F>>>` plus
  `Arc<AtomicBool>` flags pattern stays. Phase 7 adds a `color_dirty` flag
  alongside the existing `redraw_required` and `render_task_is_busy` flags.

### 9.2 Render trigger matrix

| Event                 | What runs                                | Quality                              |
| --------------------- | ---------------------------------------- | ------------------------------------ |
| Pan / zoom / click    | Full pipeline (a)→(e)                    | `sampling_level` per regulator       |
| Color edit (Phase 7+) | `recolorize_only`: `refresh_cache` + (e) | Honors current cached-field upsample |
| Space pressed         | Full pipeline (a)→(e)                    | Forced to user JSON `sampling_level` |
| Idle (no interaction) | Adaptive regulator may trigger upgrade   | `sampling_level` climbing → full     |

### 9.3 Adaptive regulator

Stays unchanged from today's
[src/core/render_quality_fsm.rs](../src/core/render_quality_fsm.rs). The
`user_interaction = true` signal continues to come from view changes. Whether
to also feed color edits into this signal is **deferred to Phase 8** — the
regulator self-tunes from observed compute time, so the choice doesn't change
the architecture, only the UX feel of "how aggressively does quality bounce
back up after the user stops dragging a slider."

### 9.4 Static-dispatch invariant

The renderer hot path is fully monomorphized over `F: Renderable`. The core
iteration helpers (`compute_raw_field`, `populate_histograms`,
`colorize_collapse`) are generic over `K: FieldKernel` and instantiated once
per fractal at compile time. There is no `dyn Renderable`, no runtime
variant check on the per-(sub)pixel hot path. The only runtime dispatch in
the system is the single `match fractal_params { … }` in
[src/cli/explore.rs](../src/cli/explore.rs) that selects which concrete `F`
to instantiate at startup.

Per-frame allocations: zero. Per-(sub)pixel allocations: zero. All buffers
(field, per-gradient histograms, per-gradient CDFs, per-gradient LUTs,
output `ColorImage`) are owned by `RenderingPipeline` or `PixelGrid` and
reused in place across renders.

### 9.5 BarnsleyFern and Serpinsky

Continue to panic in `cli::explore::explore_fractal` with "Parameter type does
not yet implement RenderWindow." Out of scope for this entire roadmap. Their
params structs are not migrated (Phase 1) and they do not implement
`Renderable` / `FieldKernel` (Phases 2/3).

---

## 10. Testing Strategy

**Bar:** strong unit tests on logical pieces; manual smoke testing on the GUI
itself. Snapshot or behavioral GUI tests are not required for this roadmap
but may be added later if a particular bug class becomes recurring.

### 10.1 What to unit-test (mandatory)

- `colorize_cell` correctness on the unified `ColorMap`:
  - All-`None` field → all output pixels equal `flat_color`.
  - Single-keyframe gradients (constant-color).
  - Boundary keyframe values (0.0 and 1.0) at LUT endpoints.
  - Multi-gradient routing: cells with `gradient_index = k` colorize
    through `gradients[k]`, not gradient 0.
  - Empty `gradients`: rejected at deserialization or construction with a
    structured error; not reachable on the colorize hot path.
- Core iteration helpers in `src/core/field_iteration.rs` against
  synthetic `FieldKernel` impls:
  - Positive `sampling_level = r`: writes exactly the `(r+1)²` cells per
    output-pixel block.
  - Block-fill `sampling_level = -m`: writes only the top-left cell of
    each `(m+1) × (m+1)` output-pixel block.
  - Histogram populate: counts equal number of `Some(_)` cells; routing
    to `histograms[k]` matches the cell's gradient index.
  - Compute receives the right `(re, im)` per subpixel (assert against
    `LinearPixelMap`).
- Fraction renormalization: edit one fraction in a 4-keyframe gradient,
  assert the others scale proportionally and the resulting positions match
  expectations. Edge cases: edit to ε, edit to 1−ε, edit to 0 (clamped),
  edit to 1.0 (clamped).
- Keyframe insertion: `+` between two existing keyframes produces the
  expected midpoint position and the expected interpolated color.
- Keyframe deletion: removing the second keyframe in a 3-keyframe gradient
  preserves positions 0.0 and 1.0 of the anchors and removes the middle one.
- serde round-trip for `ColorMap`.
- DDP `#[serde(default)]` shim: an existing pre-Phase-1 DDP JSON
  (re-created in a test fixture) still parses and produces the
  hard-coded white/black colors via the degenerate single-keyframe
  gradient.

### 10.2 What to manually smoke-test (mandatory each phase)

Per the per-phase PR checklist (§12). Same matrix as today: Windows native,
WSL2/XWayland, native Linux, mac.

### 10.3 What to leave for later

- egui snapshot tests on the editor panel rendering (would require
  `egui_kittest` or similar dev-dep).
- Synthetic-input behavioral tests (e.g. "click keyframe 2, press Delete,
  assert N-1 keyframes").
- Performance regression tests for `colorize_collapse` (initially
  benchmarked manually in Phase 7; promote to a criterion benchmark if it
  becomes a recurring concern).

---

## 11. Risks & De-risk

**JSON migration misses a file**

- **Phase:** 1
- **Mitigation:** `tests/example_parameter_validation_tests.rs` glob covers all JSONs; CI catches missed migrations.

**Schema migration changes pixel hashes**

- **Phase:** 1
- **Mitigation:** Pixel-hash regression tests gate the PR; if hashes change, the migration changed semantics — bug.

**Compute/color split breaks pixel hashes unintentionally**

- **Phase:** 2.1, 2.2
- **Mitigation:** 2.1 keeps the old path live and verifies bit equivalence
  via a dedicated test before any runtime path moves. 2.2 deletes the old
  path only after that gate passes. The single hash bump in 2.2 (block-fill
  vs linear interpolation) affects exactly one fixture and is documented
  in the commit.

**Histogram-source change in 2.3 produces visually wrong output**

- **Phase:** 2.3
- **Mitigation:** Manually eyeball-verify 2-3 PNGs per fractal family
  against the prior versions before regenerating expected hashes. If a
  family's output looks structurally wrong, debug before committing — do
  not blindly accept the new hash.

**Phase 3 refactor accidentally shifts Mandelbrot/Julia pixel hashes**

- **Phase:** 3
- **Mitigation:** Phase 3 should be invariant for Mandelbrot/Julia. Any
  hash regression on those families is a bug — investigate before
  regenerating. See [phase-3-detailed-plan.md](phase-3-detailed-plan.md)
  for the per-commit invariance gate.

**Per-root histograms produce structurally wrong Newton output**

- **Phase:** 3
- **Mitigation:** Manually eyeball-verify Newton PNGs against prior
  versions before regenerating hashes. The intended visual change is
  per-basin contrast improvement, not loss of structure.

**`colorize_collapse` too slow at 2k to be live**

- **Phase:** 7
- **Mitigation:** Benchmark over a representative populated field at 2K early in Phase 7. Falls back to Phase 8 work.

**Gradient-tab count drifts from `gradients.len()`**

- **Phase:** 5, 7
- **Mitigation:** Tab strip is a pure view of `gradients.iter().enumerate()`; no separately stored count.

**Editor state desync after gradient-tab switch**

- **Phase:** 5, 7
- **Mitigation:** Selection state resets on tab change (specified in §7.2).

**Adaptive regulator behaves badly during color editing**

- **Phase:** 7, 8
- **Mitigation:** Regulator self-tunes; if behavior is wrong, Phase 8 adjusts whether color edits feed `user_interaction`.

A wrong-shape color map cannot reach the renderer at runtime: there is one
`ColorMap` type, validated at JSON deserialization. Empty `gradients` is
the only construction-time hazard, and is rejected at construction with a
structured error.

---

## 12. Working Practices

### 12.1 CI checks (per [CLAUDE.md](../CLAUDE.md))

Before every commit:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo bench --no-run
```

Pre-commit hooks in [.claude/settings.json](../.claude/settings.json) enforce
these automatically when committing via Claude Code.

### 12.2 Branch / commit conventions

- Branches: `feature/description`, `fix/description`, `perf/description`,
  `refactor/description`.
- Commits: conventional (`feat:`, `fix:`, `perf:`, `refactor:`, `test:`,
  `docs:`, `chore:`) or imperative short titles.
- One logical change per commit.
- Include attribution for AI-assisted commits.
- Never push or open PRs without explicit user confirmation.

### 12.3 Per-phase PR checklist

- [ ] All CI green locally (fmt, clippy, test, bench --no-run).
- [ ] Unit tests added for new pure-logic pieces (per §10.1).
- [ ] Pixel-hash regression tests pass unchanged where applicable
      (Phases 1, 2, 3 especially — Phase 3 is invariant for
      Mandelbrot/Julia, regenerated for DDP/Newton).
- [ ] Manual smoke-test on various platforms (WSL, windows, mac, linux).
- [ ] If a hot path changed: `cargo bench` comparison before/after.
- [ ] If JSON schema changed: every example JSON re-loads and produces the
      same image hash (or a documented and intended pixel difference).
- [ ] Doc updates: this roadmap reflects what was actually shipped (move
      in-progress phases to "done" or amend if scope shifted).

---

## 13. Open Questions for the Implementer

These do not block any phase but should be decided as the relevant phase
lands.

1. **Drop the `QuadraticMap<T>` wrapper.** After Phase 2 it contains only
   `fractal_params: T`. Cleaner to impl `Renderable` / `FieldKernel`
   directly on `T: QuadraticMapParams + ...`. Recommendation: yes, fold in
   during Phase 3.
2. **Active gradient tab on switch.** When the active tab changes, reset
   keyframe selection. Recommended.
3. **Reuse of `paint_gradient_bar`.** The current
   [src/core/color_map_editor_ui.rs:215-241](../src/core/color_map_editor_ui.rs#L215-L241)
   already implements an artifact-free gradient bar. Keep it; lift into
   `src/core/interactive/editor.rs` rather than rewriting.
4. **Color edits → adaptive regulator?** Whether to feed color edits into
   `user_interaction = true`. Defer to Phase 8 measurement.
5. **DDP basin coloring richness.** Today DDP collapses all non-zero basins
   into one "background" bucket. After Phase 3 it has one constant-color
   gradient. Future work could expose per-basin colors by emitting the
   basin index as the gradient index in `evaluate` and shipping per-basin
   gradients in the JSON. Out of scope for this roadmap.

---

## 14. Quick Start for a New Agent

1. Read this doc end-to-end.
2. Read [src/core/user_interface.rs](../src/core/user_interface.rs) and
   [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs)
   to understand the two existing eframe apps you're unifying.
3. Read [src/core/render_window.rs](../src/core/render_window.rs) to
   understand `PixelGrid` and the existing background-render pattern.
4. Read [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs),
   [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs),
   and [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs)
   to understand the per-fractal `Renderable` / `ColorMapKind` shapes
   currently in tree (post Phase 2).
5. Confirm `cargo test` passes on `main`. Pick the next phase that hasn't
   landed.
6. Re-read §6's detail for that phase. Re-read §11 for risks specific to
   that phase. If you're picking up Phase 3, also read
   [phase-3-detailed-plan.md](phase-3-detailed-plan.md). Make a small
   first commit to keep the diff reviewable.

Good luck.
