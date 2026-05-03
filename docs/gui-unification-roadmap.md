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

| Phase | Title                         | Blast radius                                                                      |
| ----- | ----------------------------- | --------------------------------------------------------------------------------- |
| 1     | Color-map data unification    | All fractal params + every example/test JSON file                                 |
| 2     | Compute / color split         | `Renderable` trait + new `RenderingPipeline` + JSON migration to `sampling_level` |
| 3     | Unified `FractalApp` shell    | New `src/core/interactive/` module; preview only                                  |
| 4     | Color editor panel            | Editor widget + layout wiring                                                     |
| 5     | CLI + cleanup + Space-as-save | Delete legacy modules; extend snapshot behavior                                   |
| 6     | Live color sync               | `RenderingPipeline::recolorize_only` + dirty flags                                |
| 7     | Polish                        | Contents TBD post-Phase-6 measurement                                             |

Phases 1 and 2 are independent pre-work; Phase 1 is recommended first so
Phase 2 can embed each fractal's concrete color-map type directly in its
params without an interim migration.

Phases 3 → 6 are sequential. Phase 7 is opportunistic.

Each phase is a self-contained PR, bisectable, independently revertible.

### Status:

**Phase One**
Completed in 9a2e51b19a6baa7d119351d77902dba5c8aa171b.

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

Three concrete color-map shapes serve the three explorable fractal families.
Their pairing with per-cell field values is enforced **at compile time** via
associated types on a `ColorMapKind` trait — there is no runtime tuple-match
on the hot path, no `_ => panic!` arm, and no possibility of constructing a
mismatched `(field, color_map)` pair from a `Renderable` impl. Validation
happens once, at JSON deserialization; everything downstream is statically
typed.

The field shape is uniform across fractals: `Vec<Vec<Cell>>`, where the
inner `Cell` type is the per-(sub)pixel value associated with each
`ColorMapKind`. This keeps the AA-collapse loop in shared generic code and
out of per-fractal implementations.

### 5.1 Per-variant concrete types

Each color-map shape is its own struct:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ForegroundBackground {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackgroundWithColorMap {
    pub background: [u8; 3],
    pub color_map: Vec<ColorMapKeyFrame>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultiColorMap {
    pub cyclic_attractor: [u8; 3],
    pub color_maps: Vec<Vec<ColorMapKeyFrame>>,
}
```

Newton's `boundary_set_color_rgb` is intentionally absent from `MultiColorMap`:
it is dead code in the renderer ([src/fractals/newtons_method.rs:416-428](../src/fractals/newtons_method.rs#L416-L428)
never reads it) and exists today only to define the "in-set" endpoint of the
`GrayscaleSpec` shorthand, which Phase 1 drops.

### 5.2 The `ColorMapKind` trait pairs them with a per-cell type and a cache

```rust
/// Pairs a color-map type with its matching per-(sub)pixel cell type
/// and an allocation-once cache holding lookup tables and pre-converted
/// flat colors. All dispatch is statically monomorphized; there is no
/// runtime variant matching on the colorize hot path.
pub trait ColorMapKind: Sized {
    /// Per-(sub)pixel value this map consumes. CDF-normalized for the
    /// scalar variants (Phase 2 invariant: cells handed to `colorize_cell`
    /// have already been through `Renderable::normalize_field`).
    type Cell: Copy + Send + Sync;

    /// Cached form of the color map suitable for the colorize hot path.
    /// Holds preallocated `ColorMapLookUpTable`s and pre-converted
    /// `Color32` flat colors. Mutated in place by `refresh_cache`.
    type Cache: Send + Sync;

    /// One-time allocation at pipeline construction, sized for
    /// `lookup_table_count` table entries.
    fn create_cache(&self, lookup_table_count: usize) -> Self::Cache;

    /// In-place rebuild of the cache from current keyframes / flat colors.
    /// Allocation-free. Called once at startup and again whenever
    /// keyframes change (Phase 6).
    fn refresh_cache(&self, cache: &mut Self::Cache);

    /// Per-cell color lookup. Called inside the AA-collapse loop.
    fn colorize_cell(cache: &Self::Cache, cell: Self::Cell) -> [u8; 3];
}
```

Per-variant `Cell` and `Cache` types:

| Impl                     | `Cell`               | `Cache`                                                                |
| ------------------------ | -------------------- | ---------------------------------------------------------------------- |
| `ForegroundBackground`   | `Option<i32>`        | `(Color32, Color32)`                                                   |
| `BackgroundWithColorMap` | `Option<f32>`        | `(ColorMapLookUpTable, Color32)`                                       |
| `MultiColorMap`          | `Option<(f32, u32)>` | `(Vec<ColorMapLookUpTable>, Color32)` (table per root, allocated once) |

`Renderable` carries the pairing and exposes the field-compute pipeline:

```rust
pub trait Renderable: Sync + Send + SpeedOptimizer {
    type Params: Serialize + Debug;
    type ColorMap: ColorMapKind;

    /// (a) Fill the preallocated `field` buffer with raw, un-normalized
    /// values. Stride is determined by `sampling_level` (§5.6).
    fn compute_raw_field(
        &self,
        sampling_level: i32,
        field: &mut Vec<Vec<<Self::ColorMap as ColorMapKind>::Cell>>,
    );

    /// (b) Walk populated cells and insert into `histogram`. Default no-op
    /// (DDP).
    fn populate_histogram(
        &self,
        _sampling_level: i32,
        _field: &[Vec<<Self::ColorMap as ColorMapKind>::Cell>],
        _histogram: &Histogram,
    ) {}

    /// (c) Replace each populated cell's raw value with its CDF percentile
    /// in place. Default no-op (DDP).
    fn normalize_field(
        &self,
        _sampling_level: i32,
        _cdf: &CumulativeDistributionFunction,
        _field: &mut Vec<Vec<<Self::ColorMap as ColorMapKind>::Cell>>,
    ) {}

    fn color_map(&self) -> &Self::ColorMap;
    fn color_map_mut(&mut self) -> &mut Self::ColorMap;  // added in Phase 6

    // Existing: image_specification, render_options, set_image_specification,
    // write_diagnostics, params.
}
```

The hot path is generic over `F: Renderable`. Dispatch happens once, at the
top of [src/cli/explore.rs](../src/cli/explore.rs) where `match
fractal_params { … }` selects the concrete `F` to instantiate. From there
inward every call site is monomorphized.

### 5.3 The four-phase `RenderingPipeline`

A single top-level orchestrator, parameterized by `F: Renderable`, owns all
reusable buffers and runs the same four-phase pipeline for every fractal:

```rust
pub struct RenderingPipeline<F: Renderable> {
    fractal: F,
    field: Vec<Vec<<F::ColorMap as ColorMapKind>::Cell>>,
    histogram: Histogram,
    cdf: CumulativeDistributionFunction,
    color_cache: <F::ColorMap as ColorMapKind>::Cache,
    n_max_plus_1: usize,
}

impl<F: Renderable> RenderingPipeline<F> {
    pub fn render(&mut self, out: &mut egui::ColorImage, sampling_level: i32) {
        self.fractal.compute_raw_field(sampling_level, &mut self.field);     // (a)
        self.histogram.reset();
        self.fractal.populate_histogram(sampling_level, &self.field, &self.histogram); // (b)
        self.cdf.reset(&self.histogram);
        self.fractal.normalize_field(sampling_level, &self.cdf, &mut self.field);      // (c)
        self.fractal.color_map().refresh_cache(&mut self.color_cache);                 // (d, part 1)
        colorize_collapse::<F::ColorMap>(                                              // (d, part 2)
            &self.color_cache, &self.field,
            self.n_max_plus_1, sampling_level, out,
        );
    }
}
```

The CDF is a property of the field, not the keyframes — so editing keyframes
invalidates only `color_cache` (cheap rebuild via `refresh_cache`), not the
CDF or the field. Phase 6's "re-colorize-only on keyframe edit" reduces to
re-running step (d) against the cached field.

`colorize_collapse` is a generic free function (not a trait method): it walks
the row-major output `egui::ColorImage` and per output pixel reads the
corresponding `(n+1)²` block of the field, calls `C::colorize_cell` per
subpixel, and averages. Per-pixel hot path: zero allocation, zero `dyn`,
fully monomorphized.

### 5.4 Allocation strategy

All buffers are allocated **once per session** (or per window resize) on
`RenderingPipeline::new` / `resize`, never per frame:

- `field` is sized for `(n_max+1)·W × (n_max+1)·H` where `n_max+1` derives
  from the user's JSON `sampling_level` (positive AA values cap field
  size). Reallocated only on window resize.
- `histogram`, `cdf`, `color_cache` are independent of resolution; each is
  allocated once and reset/refreshed in place.
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

### 5.6 Why static typing all the way down

A `Vec<Cell>` field with a per-`ColorMapKind` `Cell` type keeps:

- **The AA-collapse loop generic.** `colorize_collapse::<C>` works for every
  fractal because the field shape is uniform. The per-cell logic dispatches
  at compile time to `C::colorize_cell`.
- **The colorize hot path allocation-free.** The cache is reused in place;
  no per-render `Vec` construction.
- **The wrong-pairing case unrepresentable.** The compiler enforces
  `field: Vec<Vec<<F::ColorMap as ColorMapKind>::Cell>>` end-to-end.
  Mismatched JSON shapes fail serde deserialization before any pipeline
  is constructed.
- **The editor static** (Phase 4): the editor widget is generic over the
  concrete `ColorMap` type, with per-variant `show_editor` implementations
  colocated with each variant's data.

The earlier-considered approach of three separate `Field` type aliases
(`BinaryBasinField`, `SmoothScalarField`, `SmoothScalarWithIndexField`)
embedded the AA-collapse logic into per-fractal code, since the loop would
have had to know the concrete field shape at every call site. The
`Vec<Cell>` framing keeps that loop in one place.

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

**Detailed plan:** see [phase-2-detailed-plan.md](phase-2-detailed-plan.md)
for trait shapes, file lists, commit-by-commit verification, and the JSON
migration script.

**Three commits in one PR:**

- **2.1 — Add machinery, parallel-to-old.** `ColorMapKind` trait, revised
  `Renderable`, new `src/core/render_pipeline.rs` with `RenderingPipeline`
  and `colorize_collapse`. Per-fractal impls of `compute_raw_field` /
  `populate_histogram` / `normalize_field` / `color_map`. Old `render_point`
  / `render_to_buffer` retained. Pixel-equivalence test against old path.
  Existing pixel-hash regression tests pass unchanged.
- **2.2 — Switch runtime paths, delete legacy.** `image_utils::render` and
  `PixelGrid::render` go through `RenderingPipeline`. Delete
  `render_point`, `render_to_buffer`, `generate_scalar_image_in_place`,
  `wrap_renderer_with_antialiasing`, `KeyframeLinearPixelInerpolation`.
  `RenderOptions::sampling_level` replaces the two old fields. Mass JSON
  migration via `scripts/migrate_phase_2_render_options.py`. The one
  fixture using `downsample_stride: 4` gets a regenerated hash (block-fill
  vs linear interp). All other pixel-hash tests pass unchanged.
- **2.3 — Histogram from full field.** `Renderable::populate_histogram`
  switches from sub-sample grid to full-field walk for
  Mandelbrot/Julia/Newton. Drop `histogram_sample_count`. Regenerate
  pixel-hash test fixtures for those three families. DDP unchanged.

**Verification per commit:**

- 2.1: `cargo test` green (existing + new equivalence tests). `cargo bench
--no-run` clean.
- 2.2: All pixel-hash regression tests still pass at unchanged hashes for
  AA-only fixtures; `mandelbrot/downsample_interpolation_regression_test`
  hash regenerated. Manual smoke-test of `cargo run -- explore` on each
  fractal family.
- 2.3: Pixel-hash regression tests pass at regenerated hashes for
  Mandelbrot/Julia/Newton (manual eyeball-verification of 2-3 PNGs per
  family before commit).

### Phase 3 — Unified `FractalApp` shell

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
in tree at this phase to keep diffs reviewable; it gets deleted in Phase 5.

**Visuals:** `panel_fill = BLACK`, `bg_stroke = NONE`, `Frame::NONE.fill(BLACK)`
on every panel — matches current explore mode and avoids border artifacts
(§4.1).

**Verification:** manual smoke-test all four fractal types on various platforms.

### Phase 4 — Color editor panel

**Goal:** add the right-side color editor panel to `FractalApp`. Editor
displays the loaded color map and allows local mutation of a **cached
copy**. The fractal preview is not affected by edits — that's Phase 6.

**Files touched:**

- `src/core/interactive/editor.rs` — new. Defines a `ColorMapEditor`
  trait (extends `ColorMapKind`) with per-variant `show_editor`
  implementations on each concrete struct. The trait method takes
  `&mut self` plus `&mut egui::Ui` and returns whether anything changed:
  ```rust
  pub trait ColorMapEditor: ColorMapKind {
      fn show_editor(&mut self, ui: &mut egui::Ui, state: &mut EditorState) -> bool;
  }
  ```
  Shared widget helpers (`show_swatch`, `show_gradient_segment`, fraction
  renormalization) live as free functions used by all three impls.
  `MultiColorMap`'s impl renders the tab strip and routes the active
  tab's gradient through the same shared helpers.
- `src/core/interactive/app.rs` — extend layout: `SidePanel::right` for
  the editor, `CentralPanel` for the preview. `FractalApp<F>` gains
  `editor_color_map: F::ColorMap` (a clone of the renderer's typed color
  map — concrete, not enum) and a small `EditorState` for selection
  (selected keyframe index, active Newton tab). Calls
  `self.editor_color_map.show_editor(ui, &mut self.editor_state)` in the
  panel — fully monomorphized, zero runtime variant matching.

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

**Local-cache lifecycle:** `editor_color_map: F::ColorMap` is initialized at
startup as a clone of `renderer.color_map()`. All editor widgets mutate only
this cache. The renderer continues to use its own (immutable, in this phase)
color map. Edits do not survive window close (no save-back to disk;
Space-as-save in Phase 5 captures the cache to a fresh timestamped JSON).

### Phase 5 — CLI + cleanup + extended Space-as-save

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
  before the snapshot render — Phase 6 turns this into a continuous flow.
- [src/core/render_window.rs](../src/core/render_window.rs) — `PixelGrid`
  may need an explicit "render at full quality, then notify" entry point to
  support the save flow (the existing `render_to_file` path can be retained
  or rewired; implementer's choice).

**Verification:** all CI green. Manual smoke-test of Space-as-save: load
example, edit colors, press Space, verify (a) overlay appears, (b) controls
locked during render, (c) timestamped JSON + PNG written to disk, (d)
re-loading the saved JSON via `cargo run -- explore <saved.json>` reproduces
the exact GUI state including colors.

### Phase 6 — Live color sync

**Goal:** color edits in the editor panel cause the fractal preview to
re-colorize live (target: <1 frame latency at 1080p).

**Approach:** `RenderingPipeline` already owns a CDF-normalized field
buffer that persists across renders (per §5.3 — there is no "cache vs
fresh field" distinction; the field _is_ the buffer). Phase 6 just adds
two dirty flags so the pipeline can run only step (d) when keyframes
change, and the full (a)→(d) sequence when the viewport changes.

**Files touched:**

- [src/core/image_utils.rs](../src/core/image_utils.rs) — add
  `color_map_mut(&mut self) -> &mut Self::ColorMap` to `Renderable`.
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
  anything changed (per Phase 4); the app uses that boolean to gate
  `color_dirty`.

**Editor cache transition:** the separate `editor_color_map: F::ColorMap` from
Phase 4 becomes redundant. The editor now mutates `renderer.color_map_mut()`
directly (still typed as `&mut F::ColorMap`, statically dispatched). The app
retains only editor _selection_ state (selected keyframe index, active Newton
tab) — the data lives on the renderer.

**AA on re-colorize.** The cached field in `RenderingPipeline` is at
whatever upsampling factor the most recent (a)→(c) pass produced. Color
edits replay (d) at the same upsampling factor, so AA-quality re-colorize
during full-quality runs comes for free. During interactive sampling
(stride > 1, sparse field), re-colorize naturally honors the same stride.

**Adaptive quality regulator interaction:** color edits trigger only
re-colorize, not re-compute, so the regulator's compute-quality scaling is not
mechanically engaged by them. Whether color edits should also feed the
`user_interaction = true` signal (so the regulator stays in "interactive
mode" and defers expensive idle-time recomputes) is a UX-feel decision to
make once Phase 6 is functional. The regulator self-tunes from observed
compute time, so neither choice is structurally wrong.

**Verification:** manual interactive testing — drag fraction sliders, click
keyframe colors, verify the preview updates within a frame or two. Benchmark
`colorize` over a representative `SmoothScalar` field at 1920×1080 to
confirm it stays under one frame at 24Hz; if not, Phase 7 must include
tweaks to the adaptive quality scaling to make the UI feel smooth.

### Phase 7 — Polish

Contents to be defined post-Phase-6 measurement. Likely candidates:

- Debouncing rapid slider drags if `colorize` proves expensive at large
  resolutions.
- Tuning the defensive `request_repaint_after` cadence.
- Visual feedback for the selected keyframe (border, highlight).
- Color picker UX refinements (RGB vs HSV, eyedropper, swatch history).
- Whether to feed color edits into the adaptive regulator's
  `user_interaction` signal.

---

## 7. Color Editor Widget Spec

### 7.1 Single-gradient editor (used by `BackgroundWithColorMap` and each

`MultiColorMap` tab)

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

### 7.2 Per-variant layout

- `ForegroundBackground` — two color picker rows: "Foreground" and
  "Background". No tabs, no keyframe list, no gradient bar.
- `BackgroundWithColorMap` — one color picker row ("Background") above one
  single-gradient editor.
- `MultiColorMap` — one color picker row ("Cyclic attractor") above a tab
  strip (one tab per entry in `color_maps`), with the active tab showing a
  single-gradient editor for that gradient. Switching tabs resets keyframe
  selection (each tab starts unselected).

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
3. **Sync color map.** Push the editor's current color map (which in Phase 6
   _is_ `renderer.color_map_mut()`; in Phase 5 was a separate
   `editor_color_map: F::ColorMap` cache) into the renderer's params for
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
- **Background thread:** `PixelGrid` worker — runs `compute_field` and
  `colorize`. The existing `Arc<Mutex<F: Renderable>>` plus
  `Arc<AtomicBool>` flags pattern stays. Phase 6 adds a `color_dirty` flag
  alongside the existing `redraw_required` and `render_task_is_busy` flags.

### 9.2 Render trigger matrix

| Event                 | What runs                                | Quality                              |
| --------------------- | ---------------------------------------- | ------------------------------------ |
| Pan / zoom / click    | Full pipeline (a)→(d)                    | `sampling_level` per regulator       |
| Color edit (Phase 6+) | `recolorize_only`: `refresh_cache` + (d) | Honors current cached-field upsample |
| Space pressed         | Full pipeline (a)→(d)                    | Forced to user JSON `sampling_level` |
| Idle (no interaction) | Adaptive regulator may trigger upgrade   | `sampling_level` climbing → full     |

### 9.3 Adaptive regulator

Stays unchanged from today's
[src/core/render_quality_fsm.rs](../src/core/render_quality_fsm.rs). The
`user_interaction = true` signal continues to come from view changes. Whether
to also feed color edits into this signal is **deferred to Phase 7** — the
regulator self-tunes from observed compute time, so the choice doesn't change
the architecture, only the UX feel of "how aggressively does quality bounce
back up after the user stops dragging a slider."

### 9.4 Static-dispatch invariant

The renderer hot path is fully monomorphized over `F: Renderable`. Every
`compute_raw_field` / `populate_histogram` / `normalize_field` call operates
on a concrete `Vec<Vec<<F::ColorMap as ColorMapKind>::Cell>>`; every
`colorize_cell` call dispatches statically to the concrete `ColorMapKind`
impl for that fractal. The AA-collapse loop in `colorize_collapse::<C>` is
generic and monomorphized per fractal at compile time. There is no enum
match, no `dyn Renderable`, no runtime variant check on the per-(sub)pixel
hot path. The only runtime dispatch in the system is the single
`match fractal_params { … }` in [src/cli/explore.rs](../src/cli/explore.rs)
that selects which concrete `F` to instantiate at startup.

Per-frame allocations: zero. Per-(sub)pixel allocations: zero. All buffers
(field, histogram, CDF, color cache, output `ColorImage`) are owned by
`RenderingPipeline` or `PixelGrid` and reused in place across renders.

### 9.5 BarnsleyFern and Serpinsky

Continue to panic in `cli::explore::explore_fractal` with "Parameter type does
not yet implement RenderWindow." Out of scope for this entire roadmap. Their
params structs are not migrated (Phase 1) and they do not implement the
`Renderable` associated types `ColorMap` / `Field` (Phase 2).

---

## 10. Testing Strategy

**Bar:** strong unit tests on logical pieces; manual smoke testing on the GUI
itself. Snapshot or behavioral GUI tests are not required for this roadmap
but may be added later if a particular bug class becomes recurring.

### 10.1 What to unit-test (mandatory)

- `ColorMapKind::colorize_into` correctness, with one test module per
  concrete impl (`ForegroundBackground`, `BackgroundWithColorMap`,
  `MultiColorMap`):
  - All-`None` field.
  - Single-keyframe gradients.
  - Boundary keyframe values (0.0 and 1.0).
  - `MultiColorMap` with empty `color_maps`: rejected at deserialization
    or at construction with a structured error; not reachable on the
    colorize hot path. (No runtime panic in `colorize_into`.)
- Fraction renormalization: edit one fraction in a 4-keyframe gradient,
  assert the others scale proportionally and the resulting positions match
  expectations. Edge cases: edit to ε, edit to 1−ε, edit to 0 (clamped),
  edit to 1.0 (clamped).
- Keyframe insertion: `+` between two existing keyframes produces the
  expected midpoint position and the expected interpolated color.
- Keyframe deletion: removing the second keyframe in a 3-keyframe gradient
  preserves positions 0.0 and 1.0 of the anchors and removes the middle one.
- serde round-trips for each concrete color-map struct (and, if implemented,
  the boundary-only `UnifiedColorMap` enum).
- `compute_field` shape correctness for each fractal: a small synthetic
  `ImageSpecification` produces a field of the expected concrete type, with
  the expected size, and the expected values at known points.
- DDP `#[serde(default)]` shim: an existing pre-Phase-1 DDP JSON
  (re-created in a test fixture) still parses and produces the
  hard-coded white/black colors.
- Negative serde tests: a JSON with a Mandelbrot params payload but a
  `MultiColorMap`-shaped color field fails to parse with a structured serde
  error. (Verifies that mismatched shapes are caught at the JSON boundary,
  not at runtime.)

### 10.2 What to manually smoke-test (mandatory each phase)

Per the per-phase PR checklist (§12). Same matrix as today: Windows native,
WSL2/XWayland, native Linux, mac.

### 10.3 What to leave for later

- egui snapshot tests on the editor panel rendering (would require
  `egui_kittest` or similar dev-dep).
- Synthetic-input behavioral tests (e.g. "click keyframe 2, press Delete,
  assert N-1 keyframes").
- Performance regression tests for `colorize` (initially benchmarked
  manually in Phase 6; promote to a criterion benchmark if it becomes a
  recurring concern).

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

**`colorize_collapse` too slow at 2k to be live**

- **Phase:** 6
- **Mitigation:** Benchmark over a representative populated field at 2K early in Phase 6. Falls back to Phase 7 work.

**Newton tab count drifts from `color_maps.len()`**

- **Phase:** 4, 6
- **Mitigation:** Tab strip is a pure view of `color_maps.iter().enumerate()`; no separately stored count.

**Editor state desync after `MultiColorMap` tab switch**

- **Phase:** 4, 6
- **Mitigation:** Selection state resets on tab change (specified in §7.2).

**Adaptive regulator behaves badly during color editing**

- **Phase:** 6, 7
- **Mitigation:** Regulator self-tunes; if behavior is wrong, Phase 7 adjusts whether color edits feed `user_interaction`.

Variant-mismatch is intentionally not on this list: the typed-pairing design
(§5) makes it unrepresentable. The only way for a wrong-shape color map to
reach the renderer is via malformed JSON, which fails serde deserialization
before any fractal object is constructed.

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
      (Phases 1, 2 especially).
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

1. **Drop the `QuadraticMap<T>` wrapper in Phase 2.2?** After 2.2 it
   contains only `fractal_params: T`. Cleaner to impl `Renderable` directly
   on `T: QuadraticMapParams + ...`. Recommendation: yes, keep small unless
   the diff sprawls.
2. **Active Newton tab on switch.** When the active tab changes, reset
   keyframe selection. Recommended.
3. **Reuse of `paint_gradient_bar`.** The current
   [src/core/color_map_editor_ui.rs:215-241](../src/core/color_map_editor_ui.rs#L215-L241)
   already implements an artifact-free gradient bar. Keep it; lift into
   `src/core/interactive/editor.rs` rather than rewriting.
4. **Color edits → adaptive regulator?** Whether to feed color edits into
   `user_interaction = true`. Defer to Phase 7 measurement.
5. **DDP basin coloring richness.** Today DDP collapses all non-zero basins
   into one "background" bucket. Future work could expose per-basin colors;
   the cleanest path is probably to add a fourth `ColorMapKind` impl
   (e.g. `IndexedBasins`) with a richer `Cell` and `Cache` type rather than
   stretching `ForegroundBackground`. Out of scope for this roadmap.

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
   to understand the three different color-map representations you're
   unifying.
5. Confirm `cargo test` passes on `main`. Pick the next phase that hasn't
   landed.
6. Re-read §6's detail for that phase. Re-read §11 for risks specific to
   that phase. Make a small first commit (e.g. just the `UnifiedColorMap`
   enum definition + tests, before any params struct changes) to keep the
   diff reviewable.

Good luck.
