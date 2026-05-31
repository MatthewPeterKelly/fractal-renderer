# GUI Unification & Color-Sync Roadmap

This document is the canonical roadmap for consolidating the project onto a
single cross-platform GUI architecture built on `eframe`/`egui`, and for
delivering a unified interactive experience that combines fractal exploration
with live color-map editing.

**Audience:** a new agent or contributor picking up the GUI work. This doc is
self-contained — no prior conversation context is needed.

**Scope:** everything from the current state (renderer pre-work complete)
through to "live color edits visibly synced into the fractal preview" plus a
deliberate, restorable snapshot-to-disk. Out of scope: parameter inspector
panels, live fractal-type switching, support for fractal types not already
explorable today (BarnsleyFern, Serpinsky), DDP per-basin coloring, undo/redo,
drag-and-drop on keyframes, and save-back to the original input JSON.

> **History.** Phases A/B (port `explore` to `eframe`; remove `pixels`; Rust
> edition 2024 + `eframe` 0.34) and Phases 1–3 (renderer-architecture pre-work:
> color-map data unification, compute/color split, pipeline unification) have
> all shipped. See §2 for what that means in practice. The earlier (pre-Phase-3)
> data shapes are preserved in git history (commits `4199c23`, `0caa21c`,
> `8008aff`, `56ad860`, `f9905b5`, `7308de0`) and are not reproduced here.

---

## 1. End State Vision

The binary ships with exactly two modes:

1. **Headless render mode** (`fractal-renderer render <params.json>`) —
   unchanged. Writes images to disk based on a params JSON file. No GUI.
2. **Interactive mode** (`fractal-renderer explore <params.json>`) — a single
   unified GUI window that combines:
   - Fractal preview (pan/zoom/click).
   - Color-map editor (per-keyframe color + position, multi-color-map tabs for
     Newton's per-root palettes).
   - Live preview updates as colors are edited.
   - Snapshot-to-disk via Space, capturing both the fully-rendered image and
     the synced parameter JSON (the saved JSON, when re-loaded, restores the
     GUI to exactly the captured state).

Built entirely on `eframe` (egui's official framework), with a background
render thread feeding a `TextureHandle` for live updates.

**What still disappears over the remaining phases:**

- The `color_swatch` CLI subcommand and its supporting code (Phase 6).
- The standalone `color-gui-demo` example — its useful pieces are folded into
  `explore` (Phase 6).
- The legacy explore app ([src/core/user_interface.rs](../src/core/user_interface.rs))
  and the demo color editor ([src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs)),
  absorbed into a unified `src/core/interactive/` module (Phases 4 and 6).

The `pixels` crate and direct `winit` usage were already removed in Phases A+B.

---

## 2. What's Already Shipped

The entire renderer architecture this roadmap depends on is **done**. The GUI
work proper picks up from a clean, unified foundation.

### Platform & dependencies

```toml
edition = "2024"
eframe = { version = "0.34", default-features = false, features = ["wgpu", "x11", "wayland"] }
egui = "0.34"
# pixels and direct winit have been removed entirely.
```

### Unified color + compute architecture

The three explorable fractal families used to carry structurally different
color representations (Mandelbrot/Julia: one gradient + flat background; DDP:
hard-coded white/black; Newton: boundary + attractor colors + a color-map-spec
enum). **They are now unified** behind one data model (see §3 for full detail):

- A single `ColorPalette` type (`background_color` + `Vec<ColorMap>`, where
  `ColorMap = Vec<ColorMapKeyFrame>`) lives in
  [src/core/color_map.rs](../src/core/color_map.rs). Every fractal embeds a
  `pub color: ColorPalette`.
- A `FieldKernel::evaluate(point) -> Option<(f32, u32)>` trait
  ([src/core/field_iteration.rs](../src/core/field_iteration.rs)) is the only
  per-point math a fractal implements. `Renderable`
  ([src/core/image_utils.rs](../src/core/image_utils.rs)) adds the housekeeping
  surface (params, image spec, histogram sizing, `color_palette()` /
  `color_palette_mut()`).
- A single `RenderingPipeline<F>`
  ([src/core/render_pipeline.rs](../src/core/render_pipeline.rs)) owns all
  reusable buffers and runs the four-step pipeline: `compute_raw_field` →
  `populate_histograms` → `ColorPaletteCache::refresh_after_compute_pass` →
  `colorize_collapse_unified`. The field stays raw end-to-end; CDF lookup
  happens inside `colorize_cell` at colorize time. Newton uses per-root
  histograms / CDFs.
- Anti-aliasing and block-fill traversal collapse to a single signed
  `sampling_level: i32` knob driven by the adaptive regulator (§3.5).

### The key consequence for the GUI work

**All four explorable fractal types already run through one generic app.**
Mandelbrot, Julia, and DDP dispatch in
[src/cli/explore.rs](../src/cli/explore.rs) into `user_interface::explore::<F>`;
Newton's [explore_fractal](../src/fractals/newtons_method.rs) merely picks its
`SystemType` and calls the **same** `user_interface::explore`. Underneath, every
type flows through `ExploreApp` → [`PixelGrid<F>`](../src/core/render_window.rs)
→ `RenderingPipeline<F>`.

So the "unify all four fractal types into one app" goal is **already met at the
render level**. The remaining work is: (1) reorganize that app into a
`src/core/interactive/` module and bolt on a color editor panel wired live; (2)
make Space a deliberate, restorable save; (3) delete the now-dead `color_swatch`
and demo modules.

Two pieces of Phase-7 plumbing also already exist: `Renderable::color_palette_mut()`
is defined (currently `#[allow(dead_code)]`), and because the field is raw and
the CDFs derive from it (not from the keyframes), a color-only re-render is just
"refresh the LUTs and re-walk the field" — no recompute, no CDF rebuild needed.

### Phase status at a glance

| Phase | Title                                      | Status                           |
| ----- | ------------------------------------------ | -------------------------------- |
| A     | Port `explore` to eframe; remove pixels    | ✅ shipped (`81df7b6`)           |
| B     | Rust edition 2024 + eframe 0.34            | ✅ shipped (`0c67ddd`)           |
| 1     | Color-map data unification                 | ✅ shipped (`9a2e51b`)           |
| 2     | Compute / color split                      | ✅ shipped (`4199c23`…`8008aff`) |
| 3     | Pipeline unification & per-root colors     | ✅ shipped (`56ad860`…`7308de0`) |
| 4     | Unified `interactive` module + live editor | ⬜ next (§5)                     |
| 5     | Gated, restorable Space-as-save            | ⬜ §5                            |
| 6     | CLI + cleanup (retire dead code)           | ⬜ §5                            |
| 7     | Polish                                     | ⬜ §5 (opportunistic)            |

Phases 4 → 6 are sequential; each is a self-contained PR, bisectable and
independently revertible. Phase 7 is opportunistic.

BarnsleyFern and Serpinsky remain out of scope: they panic in
[cli::explore::explore_fractal](../src/cli/explore.rs) with "Parameter type does
not yet implement RenderWindow", their params are not color-migrated, and they
do not implement `FieldKernel` / `Renderable`.

---

## 3. Data Model

A single uniform color-palette type, a single uniform cell shape, and a
four-step pipeline serve every fractal family. Per-fractal customization reduces
to one method (`FieldKernel::evaluate(point) -> Option<(f32, u32)>`) plus static
config.

The field shape is `Vec<Vec<Option<(f32, u32)>>>` — the `f32` is the raw scalar
value (smooth iteration count, basin marker, etc.) and the `u32` is the
_color-map index_ picking which color map to colorize through.
Mandelbrot/Julia/DDP always emit color-map index 0; Newton emits the root index.
The field stays raw end-to-end — there is no normalize pass; CDF percentile
lookup happens inside `colorize_cell`.

### 3.1 The unified `ColorPalette` type

A single color map is itself a `Vec<ColorMapKeyFrame>`, so the outer container is
"a palette of color maps plus a background":

```rust
/// A single color map: the keyframes that get interpolated to colorize
/// one channel of a fractal.
pub type ColorMap = Vec<ColorMapKeyFrame>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorPalette {
    /// Color used for cells whose evaluation produced no scalar (Mandelbrot
    /// in-set, DDP out-of-basin, Newton non-converging).
    pub background_color: [u8; 3],
    /// One color map per "channel". Mandelbrot/Julia/DDP have
    /// `color_maps.len() == 1`; Newton has one entry per root. The `u32` in
    /// each cell indexes into this vec. Must be non-empty (enforced at
    /// deserialization).
    pub color_maps: Vec<ColorMap>,
}
```

DDP's degenerate "all foreground" case is encoded as a single-color-map palette
whose color map is constant (the foreground color repeated at `query=0.0` and
`query=1.0`), via a `#[serde(default)]` on the DDP params.

`ColorPalette` exposes `create_cache` as an inherent method; the pipeline
operates over a concrete `ColorPalette` (no generic parameter, no trait). The
cache owns the per-color-map histograms and exposes a single atomic
`refresh_after_compute_pass` entry point that rebuilds every downstream-visible
piece of state at once, so the colorize step can never observe a half-updated
cache.

```rust
impl ColorPalette {
    /// Allocate the cache once at pipeline construction.
    pub fn create_cache(
        &self,
        histogram_bin_count: usize,
        histogram_max_value: f32,
        lookup_table_count: usize,
    ) -> ColorPaletteCache;
}

pub struct ColorPaletteCache {
    /// Per-color-map histogram. Reset by the pipeline before each compute
    /// pass; filled in `field_iteration::populate_histograms`. Read back
    /// inside `refresh_after_compute_pass` to rebuild the CDFs.
    pub histograms: Vec<Histogram>,
    // Private: rebuilt atomically by `refresh_after_compute_pass`.
    cdfs: Vec<CumulativeDistributionFunction>,
    lookup_tables: Vec<ColorMapLookUpTable>,
    background: Color32,
}

impl ColorPaletteCache {
    /// Read-only access to the per-color-map CDFs; the colorize hot path
    /// reads them directly from `colorize_cell`.
    pub fn cdfs(&self) -> &[CumulativeDistributionFunction];

    /// Zero every histogram bin. Call before each
    /// `field_iteration::populate_histograms` invocation.
    pub fn reset_histograms(&mut self);

    /// Rebuild CDFs from `self.histograms`, refresh LUTs from `palette`'s
    /// current keyframes, and refresh the cached `background` color.
    /// Allocation-free; re-runs after keyframe edits (Phase 4 live sync).
    pub fn refresh_after_compute_pass(&mut self, palette: &ColorPalette);
}

/// Per-cell colorize. Statically dispatched, called inside the AA-collapse
/// loop. CDF lookup + LUT lookup happen here, in color space.
#[inline]
pub fn colorize_cell(cache: &ColorPaletteCache, cell: Option<(f32, u32)>) -> [u8; 3];
```

### 3.2 The `Renderable` / `FieldKernel` split

`FieldKernel` is the small surface every fractal must implement —
domain-specific scalar evaluation at one point. `Renderable` extends it with
housekeeping. All AA / block-fill iteration logic lives in core helpers generic
over `K: FieldKernel`; no fractal duplicates the parallel-iter skeleton.

```rust
/// Domain-specific per-point evaluation.
pub trait FieldKernel: Sync + Send {
    /// Evaluate the scalar field at one real-space point.
    /// Returns `Some((value, color_map_index))` or `None` for "no value".
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)>;
}

/// Pipeline plumbing. Adds the things every fractal needs that aren't
/// per-point math.
pub trait Renderable: Sync + Send + SpeedOptimizer + FieldKernel {
    type Params: Serialize + Debug;

    fn image_specification(&self) -> &ImageSpecification;
    fn set_image_specification(&mut self, image_specification: ImageSpecification);
    fn render_options(&self) -> &RenderOptions;
    fn params(&self) -> &Self::Params;
    fn write_diagnostics<W: Write>(&self, writer: &mut W) -> io::Result<()>;
    fn color_palette(&self) -> &ColorPalette;
    fn color_palette_mut(&mut self) -> &mut ColorPalette; // already present; used by Phase 4 live sync

    /// Histogram capacity in bins per color map.
    fn histogram_bin_count(&self) -> usize;
    /// Maximum scalar value the histogram can absorb.
    fn histogram_max_value(&self) -> f32;
    /// LUT resolution per color map.
    fn lookup_table_count(&self) -> usize;
}
```

The hot path is generic over `F: Renderable`. Dispatch happens once, in the
`match fractal_params { … }` at the top of
[src/cli/explore.rs](../src/cli/explore.rs) (and Newton's inner
`SystemType` match) that selects the concrete `F` to instantiate. From there
inward every call site is monomorphized.

### 3.3 The four-step `RenderingPipeline`

A single top-level orchestrator, parameterized by `F: Renderable`, owns all
reusable buffers. Only step (a) is fractal-specific — the rest is shared core
code:

```rust
pub struct RenderingPipeline<F: Renderable> {
    fractal: F,
    field: Vec<Vec<Option<(f32, u32)>>>,
    color_cache: ColorPaletteCache, // histograms + CDFs + LUTs + background
    n_max_plus_1: usize,
}

impl<F: Renderable> RenderingPipeline<F> {
    pub fn render(&mut self, out: &mut egui::ColorImage, sampling_level: i32) {
        // (a) Fill the field with raw values via the fractal's FieldKernel.
        field_iteration::compute_raw_field(
            self.fractal.image_specification(),
            self.n_max_plus_1, sampling_level, &self.fractal, &mut self.field);

        // (b) Bin into the cache's per-color-map histograms.
        self.color_cache.reset_histograms();
        field_iteration::populate_histograms(
            self.n_max_plus_1, sampling_level, &self.field,
            &mut self.color_cache.histograms);

        // (c) Atomically rebuild CDFs (from histograms), LUTs (from keyframes),
        // and the background color.
        self.color_cache.refresh_after_compute_pass(self.fractal.color_palette());

        // (d) Walk field; CDF + LUT lookup per cell; AA-average per output pixel.
        field_iteration::colorize_collapse_unified(
            &self.color_cache, &self.field,
            self.n_max_plus_1, sampling_level, out);
    }
}
```

This shape is what makes live color editing cheap (Phase 4):

- **Keyframe edits** invalidate only the LUTs (and optionally the background).
  Re-run a focused refresh + (d); skip (a)/(b) and the CDF rebuild. The
  `recolorize_only` fast path lives here.
- **No race between compute and colorize**: the field is only ever written by
  (a) and only ever read by (b)/(d).
- **Per-root histograms come for free**: Newton bins into separate histograms
  per root; the others reduce to a single histogram.

### 3.4 Allocation strategy

All buffers are allocated **once per session** (or per window resize), never per
frame:

- `field` is sized for `(n_max+1)·W × (n_max+1)·H` where `n_max+1` derives from
  the user's JSON `sampling_level`. Reallocated only on window resize.
- The per-color-map `histograms`, CDFs, and LUTs are resolution-independent;
  each is allocated once and reset/refreshed in place. Their vec lengths come
  from `fractal.color_palette().color_maps.len()` at construction.
- The output `egui::ColorImage` is owned by `PixelGrid`, sized to `[W, H]`,
  reallocated only on resize.

Per-frame allocations: zero. Per-(sub)pixel allocations: zero.

### 3.5 The `sampling_level` model

`RenderOptions` collapses the old `subpixel_antialiasing: u32` and
`downsample_stride: usize` into a single `sampling_level: i32`:

| `sampling_level` | Field cells per output pixel | Output pixels per field cell | Mode                    |
| ---------------- | ---------------------------- | ---------------------------- | ----------------------- |
| `+n` (n > 0)     | `(n+1)²`                     | 1                            | Anti-alias              |
| `0`              | 1                            | 1                            | Baseline                |
| `−n` (n > 0)     | sparse                       | `(n+1)²`                     | Block-fill (downsample) |

The JSON-supplied `sampling_level` is the **maximum** the pipeline ever uses
(the cap that determines field-buffer size). The
`AdaptiveOptimizationRegulator` drives the **runtime** value passed to
`RenderingPipeline::render`: at full quality it equals the user value; under
interactive load it drops toward 0 and into the negative range as needed.
Block-fill is nearest-neighbor / zero-order hold.

### 3.6 Why one uniform color-palette type

A single `ColorPalette` shape with `Vec<ColorMap>` keeps:

- **All AA / block-fill iteration in core.** Three core helpers
  (`compute_raw_field`, `populate_histograms`, `colorize_collapse_unified`) each
  consume the same `Vec<Vec<Option<(f32, u32)>>>` field and the same
  `&ColorPaletteCache`. No fractal touches the parallel-iter skeleton.
- **The colorize hot path allocation-free.** The cache is reused in place.
- **One LUT shape, one CDF shape, one cell shape.** Mandelbrot/Julia/DDP reduce
  to `color_maps.len() == 1`; Newton uses N>1. N=1 is the same path as N=many,
  just with a unit-length vec.
- **The editor static** (Phase 4): the editor widget operates on a single
  concrete `ColorPalette` type. Per-fractal customization lives in the
  per-fractal renderer, not the editor.

---

## 4. Hard Constraints & Cross-Platform Learnings

These are preserved from cross-platform work during Phases A+B. They remain
relevant to any GUI work going forward. The current explore app
([src/core/user_interface.rs](../src/core/user_interface.rs)) and the shared
[src/core/eframe_support.rs](../src/core/eframe_support.rs) already apply them;
the unified `interactive` module must carry them forward.

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
   `Frame::NONE.fill(Color32::BLACK)`.
3. Manual 1-logical-pixel strokes at fractional x-positions anti-alias across
   two physical pixels (e.g. a gradient bar drawn with `line_segment`).
   → Fix: use `painter.rect_filled` with contiguous rectangles instead (the
   technique in `color_map_editor_ui::paint_gradient_bar`).

### 4.2 Resize event drops on WSL/XWayland

**Symptom:** window appears not to resize, or content stops updating when the
user drags the window edge.

**Mitigation:** call `ctx.request_repaint_after(IDLE_TICK)` at the end of every
`update()` so eframe re-polls surface size every ~100ms. Already implemented in
the explore app.

### 4.3 egui panel width locking

`SidePanel::exact_width(w)` clamps `width_range` to `w..=w`, making the panel
non-resizable even though the resize drag handle still renders. **Use
`default_width(w).width_range(min..=max)` instead.**

### 4.4 Adaptive device limits

`wgpu_core` rejects requests for limits the GPU doesn't expose
(`max_color_attachments`, etc.). Virtualized and software drivers
(WSL/XWayland, llvmpipe) routinely expose only 2–4. Solution lives in
[src/core/eframe_support.rs](../src/core/eframe_support.rs): clone the adapter's
own limits into the device descriptor. Every interactive entry point goes
through `wgpu_native_options`; the unified app must too.

### 4.5 Wgpu version coupling (no longer load-bearing, but worth knowing)

`wgpu_core` exports `#[no_mangle]` C symbols. Two versions in the same binary →
linker error. This is why `pixels` had to be removed before `eframe` could be
upgraded. The dep tree is now clean (`eframe 0.34` only); future upgrades within
the eframe family are unconstrained.

---

## 5. Remaining Work — the PR Sequence

Four PRs remain. Each is a self-contained, bisectable PR. The editor is wired
**live from the first PR** — there is no intermediate "edit a copy that does
nothing" step, because `color_palette_mut()` already exists and the raw-field
architecture makes a color-only re-render trivial (§3.3).

| PR      | Title                                            | Blast radius                                                       |
| ------- | ------------------------------------------------ | ------------------------------------------------------------------ |
| Phase 4 | Unified `interactive` module + live color editor | New `src/core/interactive/`; `recolorize_only`; `color_dirty` flag |
| Phase 5 | Gated, restorable Space-as-save                  | Save state machine; full-`FractalParams` serialization             |
| Phase 6 | CLI + cleanup (retire dead code)                 | Delete `color_swatch`, demo example, demo editor module            |
| Phase 7 | Polish                                           | Contents TBD post-Phase-5 measurement                              |

### Phase 4 — Unified `interactive` module + live color editor

**Goal:** move the explore app into a new `src/core/interactive/` module, add
the right-side color editor panel, and wire edits to the preview live. After
this PR, `explore <params.json>` shows the fractal on the left and an editable
color palette on the right; editing a keyframe recolors the preview within a
frame or two. Pan/zoom/click/Space all behave as today (the new Space flow is
Phase 5).

This is the largest PR; split it into reviewable commits — (1) module reorg,
no behavior change; (2) editor widget; (3) live-sync plumbing; (4) key remaps.

**Files touched:**

- **New `src/core/interactive/mod.rs`** — module root; re-exports `explore`.
- **New `src/core/interactive/app.rs`** — `FractalApp<F: Renderable>`, lifted
  from `ExploreApp` in [src/core/user_interface.rs](../src/core/user_interface.rs).
  Layout: `CentralPanel` (preview) + `SidePanel::right` (editor). Holds an
  `EditorState`. Visuals per §4.1 (`panel_fill = BLACK`, `bg_stroke = NONE`,
  `Frame::NONE.fill(BLACK)` on both panels).
- **New `src/core/interactive/editor.rs`** — the palette editor (full widget
  spec in §6):

  ```rust
  /// Render the editor for `palette`, mutating it in place. Returns `true`
  /// if any keyframe / fraction / background color changed this frame.
  pub fn show_palette_editor(
      palette: &mut ColorPalette,
      ui: &mut egui::Ui,
      state: &mut EditorState,
  ) -> bool;

  pub struct EditorState {
      pub selected_keyframe: Option<usize>,
      pub active_color_map: usize,
  }
  ```

  Tab strip suppressed when `color_maps.len() == 1`; otherwise "Root 0", "Root
  1", … select which color map the keyframe widgets edit. Lift the artifact-free
  gradient bar from `color_map_editor_ui::paint_gradient_bar` (open question
  §12.3) rather than rewriting it.

- **Delete [src/core/user_interface.rs](../src/core/user_interface.rs)** (its
  content moves into `interactive/`). Update
  [src/core/mod.rs](../src/core/mod.rs): remove `pub mod user_interface;`, add
  `pub mod interactive;`.
- **[src/cli/explore.rs](../src/cli/explore.rs)** and Newton's
  [explore_fractal](../src/fractals/newtons_method.rs) — retarget the
  `user_interface::explore` calls to `interactive::explore`.
- **[src/core/render_pipeline.rs](../src/core/render_pipeline.rs)** — add a
  color-only re-render entry point:
  ```rust
  /// Re-colorize the existing field after a keyframe edit. Skips (a)/(b);
  /// refreshes LUTs + background, then re-walks the field. The histograms
  /// still hold the last compute pass's counts (they are only reset at the
  /// start of a full `render`), so `refresh_after_compute_pass` reproduces
  /// identical CDFs — correct and cheap.
  pub fn recolorize_only(&mut self, out: &mut egui::ColorImage, sampling_level: i32);
  ```
  (Optional: a leaner `ColorPaletteCache::refresh_luts_and_background` that skips
  the CDF rebuild entirely. Either is correct; benchmark before optimizing.)
- **[src/core/render_window.rs](../src/core/render_window.rs)** — `PixelGrid`
  gains `color_dirty: Arc<AtomicBool>` and a private `recolorize()` mirroring
  `render()` but calling `recolorize_only`. In `update()`, when `color_dirty` is
  set (and no render is busy and no view change is pending) spawn a recolorize
  task; view/field changes take priority. Stash `initial_color_palette:
ColorPalette` (cloned in `new`) so `reset()` can restore it (R-resets-colors,
  §6.3).
- **Key remaps (§6.3) in `app.rs`:** remove the Esc-as-quit binding; `Esc`
  clears `selected_keyframe` (no-op when nothing selected); `Delete` removes the
  selected keyframe (no-op on the first/last anchors); `R` resets view **and**
  palette; `Q` / `Ctrl+C` quit. When `show_palette_editor` returns `true`, write
  through `color_palette_mut()`, set `color_dirty`, and `ctx.request_repaint()`.

**Verification:** `cargo test` — pixel-hash regression tests unchanged (no
render-math change). Manual smoke-test all four fractals: pan/zoom/click/Space
still work; editing a keyframe recolors the preview live; Newton shows one tab
per root; `Esc` clears selection (does **not** quit); `Delete` removes a
non-anchor keyframe; `R` resets view + colors.

### Phase 5 — Gated, restorable Space-as-save

**Goal:** replace today's fire-and-forget snapshot with the deliberate "publish
this exact state" flow specified in §7 — input locked, forced to full quality,
and writing a **reloadable** parameter file alongside the PNG.

**Files touched:**

- **`src/core/interactive/app.rs`** — implement the §7.2 state machine: on
  Space, set `save_in_progress`, draw the overlay, suppress all input, force the
  `AdaptiveOptimizationRegulator` to level 0 (caching the prior level so the next
  user interaction restores responsiveness), trigger a full-quality render, block
  (overlay up) until it completes, then write JSON + PNG, then unlock.
- **[src/core/render_window.rs](../src/core/render_window.rs)** — replace/extend
  `render_to_file` with a snapshot path that serializes a **reloadable
  `FractalParams`** (the tagged enum) plus the PNG, not just the
  `image_specification` as today.

**Design wrinkle (must be solved here):** the saved JSON must round-trip through
`explore <saved.json>`, i.e. it must be the tagged
[`FractalParams`](../src/fractals/common.rs) shape (`{"Mandelbrot": { … }}`).
But `interactive::explore::<F>` is monomorphized over the **inner** `F::Params`
and the `FractalParams` variant tag is dropped at the
[src/cli/explore.rs](../src/cli/explore.rs) dispatch. Thread the variant back
through — e.g. pass a `rewrap: fn(F::Params) -> FractalParams` (or a variant
discriminant) from the dispatch site, which already knows the concrete type,
into `interactive::explore` / `FractalApp`. The serialized params must also
reflect the live `image_specification` and the current (possibly edited)
`ColorPalette`. (Note: today's headless `render` writes the bare inner params,
not a tagged `FractalParams`; closing this gap for the snapshot is part of this
phase.)

**Verification:** edit colors, press Space → (a) overlay appears, (b) input
locks during the render, (c) a timestamped JSON + PNG land on disk, (d) `explore
<saved.json>` restores the exact view + colors + quality + fractal type, and the
reloaded preview matches the saved PNG.

### Phase 6 — CLI + cleanup (retire dead code)

**Goal:** delete the dead `color_swatch` path and the demo modules now that
`explore` subsumes them.

**Files touched (deletions):**

- [src/cli/color_swatch.rs](../src/cli/color_swatch.rs) — delete.
- [src/cli/args.rs](../src/cli/args.rs) — remove the `ColorSwatch` variant from
  `CommandsEnum`.
- [src/cli/mod.rs](../src/cli/mod.rs) — remove `pub mod color_swatch;`.
- [src/main.rs](../src/main.rs) — remove the `ColorSwatch` dispatch arm and the
  `use cli::color_swatch::generate_color_swatch` import.
- [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs) —
  delete; its useful gradient-bar technique was lifted into
  `interactive/editor.rs` in Phase 4.
- [src/core/mod.rs](../src/core/mod.rs) — remove the deleted module decl.
- [examples/color-gui-demo/](../examples/color-gui-demo/) and
  [examples/visualize-color-swatch-rainbow/](../examples/visualize-color-swatch-rainbow/)
  — delete.
- [examples/common/mod.rs](../examples/common/mod.rs) — drop the
  `color_swatch::generate_color_swatch` and `color_map_editor_ui::run_color_editor`
  imports and any helper functions that reference them.

**Verification:** `cargo build` / `cargo test` green;
[tests/example_parameter_validation_tests.rs](../tests/example_parameter_validation_tests.rs)
still globs cleanly after the example deletions.

### Phase 7 — Polish

Contents to be defined post-Phase-5 measurement. Likely candidates:

- Debouncing rapid slider drags if `colorize_collapse_unified` proves expensive
  at large resolutions.
- Visual feedback for the selected keyframe (border, highlight).
- Color picker UX refinements (RGB vs HSV, eyedropper, swatch history).
- Whether to feed color edits into the adaptive regulator's `user_interaction`
  signal (§8.3).
- Tuning the defensive `request_repaint_after` cadence.

---

## 6. Color Editor Widget Spec

### 6.1 Single-color-map editor (used by each tab)

**Read-only displays:**

- Vertical sequence of color cells, one per keyframe. Each cell is a small
  filled rectangle (~32×32px) showing the keyframe's RGB. Selectable.
- A horizontal gradient bar showing the full color map as currently configured.
  Read-only — no drag-to-edit, no click-to-insert.

**Mutable handles:**

- Between each pair of adjacent keyframes: a `+` button and a `DragValue`
  showing the _fraction_ of the gradient occupied by that segment (the
  difference between the two adjacent keyframe positions).
- Inline color picker (egui's `color_picker_color32`), permanently visible at
  the bottom of the panel.

**Interactions:**

- **Click a color cell** → that keyframe becomes the selected keyframe. The
  inline color picker switches to editing its color. Live: every picker change
  writes into the keyframe's `rgb_raw`.
- **`Delete` key** while a keyframe is selected → that keyframe is removed;
  selection clears; the picker returns to idle. The first and last keyframes
  (positions 0.0 and 1.0) are anchors and cannot be deleted (`Delete` is a no-op
  on them).
- **`Escape` key** → clears keyframe selection. Picker returns to idle.
  **`Escape` does not exit the application.**
- **`+` button** between two adjacent keyframes → inserts a new keyframe at the
  midpoint of that segment. Default color: linearly interpolated between the two
  adjacent keyframes (so insertion is initially invisible until the user edits
  the new keyframe). The `+` button does not appear before the first keyframe or
  after the last.
- **Edit a fraction `DragValue`** → that fraction adopts the new value; the
  _other_ fractions are scaled proportionally so the sum stays 1.0; keyframe
  positions are recomputed from the resulting fractions. Each fraction is clamped
  to `[ε, 1.0]` (with `ε ≈ 0.001`) to prevent any segment from collapsing to
  zero width.

### 6.2 Layout

The unified `ColorPalette` always renders the same widget shape:

```
┌─────────────────────────────────────────────┬──────────────────────┐
│                                             │ Color Map            │
│                                             │ ───────────────────  │
│                                             │ [Newton tabs only:]  │
│                                             │ │Root 0│Root 1│...│  │
│                                             │ ───────────────────  │
│         (fractal preview, central)          │ Background:          │
│                                             │  [swatch]            │
│                                             │ ───────────────────  │
│                                             │ Keyframes:           │
│                                             │  [color cell #0]     │
│                                             │  [+] [0.25]          │
│                                             │  [color cell #1]     │
│                                             │  ...                 │
│                                             │ ───────────────────  │
│                                             │ [gradient bar]       │
│                                             │ ───────────────────  │
│                                             │ [color picker]       │
└─────────────────────────────────────────────┴──────────────────────┘
```

- One color picker row at top, labeled per-fractal (`Background` for
  Mandelbrot/Julia/DDP; `Cyclic attractor` for Newton), that edits
  `background_color`.
- A tab strip with one tab per entry in `color_maps`. When `color_maps.len() ==
1` the tab strip is suppressed and the lone color map's editor renders
  directly. Otherwise tabs are "Root 0", "Root 1", … and the active tab shows
  the single-color-map editor for that color map. Switching tabs resets keyframe
  selection.

### 6.3 Application keys (interactive mode)

| Key                 | Behavior                                                                         |
| ------------------- | -------------------------------------------------------------------------------- |
| Arrow keys          | Pan view (existing).                                                             |
| W / S               | Zoom in / out (existing).                                                        |
| A / D (with no W/S) | Fast zoom in / out (existing).                                                   |
| R                   | Reset to initial view (existing) and color palette (new).                        |
| Mouse left-click    | Recenter view on clicked point in the fractal preview (existing).                |
| Space               | Save snapshot — see §7.                                                          |
| Q                   | Exit application.                                                                |
| Ctrl+C              | Exit application (terminal default).                                             |
| Esc                 | Clear keyframe selection. **No-op when no keyframe is selected.** Does not exit. |
| Delete              | Remove selected keyframe (no-op for first/last anchors).                         |

The Esc-as-quit binding in today's
[src/core/user_interface.rs](../src/core/user_interface.rs) must be removed
during Phase 4.

### 6.4 Out of scope

- Drag-and-drop on the gradient bar or color cells.
- Arrow-key / Tab navigation between keyframes (click only).
- Undo / redo.
- Adding or removing entire color maps (e.g. changing Newton's root count).
- Reordering keyframes (positions are derived from positive fractions, so order
  is stable).

---

## 7. Space-as-Save Spec

Pressing Space initiates a deliberate "publish this exact state" action. Unlike
today's fire-and-forget snapshot, the new flow is gated on a full-quality render
and locks input (with user feedback) until complete.

### 7.1 State machine

```
Idle ──Space pressed──► Saving ──save complete──► Idle
                          │
                          ├── overlay shown
                          ├── input locked
                          ├── adaptive regulator forced to level 0
                          └── re-render in flight
```

### 7.2 Step sequence

1. **Lock & overlay.** Set `save_in_progress = true`. Display a feedback overlay
   (translucent panel, "Saving snapshot…"). All input is suppressed for the
   duration: pan/zoom keys, click-to-center, Space (debouncing double-press),
   Esc, color edits.
2. **Force quality to default.** Reset `AdaptiveOptimizationRegulator` so the
   next render uses full user-specified quality, not whatever degraded state
   interactive use pushed it to. Cache the current value so the next user
   interaction restores responsiveness immediately.
3. **Sync color palette.** In Phase 4+, the editor already mutates
   `renderer.color_palette_mut()` directly, so the renderer's params are
   current; this step just confirms there are no pending edits to flush before
   serialization.
4. **Render to GUI.** Background thread runs the full pipeline at full quality
   and swaps the result into the preview texture. The save flow blocks (overlay
   up) until the render completes.
5. **Save params to disk.** Serialize the now-synced **tagged `FractalParams`**
   (including the embedded `ColorPalette` and the current
   `image_specification`) to `<prefix>_<datetime>.json`. See the §5 Phase-5
   design wrinkle: the variant tag must be threaded back in for the file to be
   reloadable.
6. **Save image to disk.** Write the just-rendered buffer to
   `<prefix>_<datetime>.png`. Pixels match what's on screen.
7. **Unlock.** Clear `save_in_progress`; remove overlay; resume input.

### 7.3 Restorability invariant

Calling `cargo run -- explore <saved.json>` on the file produced by step 5 must
restore the GUI to _exactly_ the state it was in when Space was pressed: the
same view bounds, the same color palette (including any edits), the same render
quality parameters, the same fractal type. The pixel hash of the rendered
preview should match the saved PNG.

### 7.4 Comparison to current behavior

Today's Space ([src/core/render_window.rs](../src/core/render_window.rs),
`render_to_file`) snapshots whatever the display buffer happens to contain and
writes only the `image_specification` — so the saved JSON is **not** a reloadable
params file. The new flow is the opposite: deliberate "publish this exact state"
gated on a full-quality render, writing a fully reloadable params file. The user
accepts a brief block in exchange for guaranteed fidelity between what's on
screen, what's written to disk, and what re-loads next time.

---

## 8. Threading & Adaptive Quality

### 8.1 Thread layout

- **UI thread:** the eframe app — layout, input, editor mutations.
- **Background thread:** `PixelGrid` worker — runs `RenderingPipeline::render`
  and (Phase 4+) `recolorize_only`. The existing `Arc<Mutex<RenderingPipeline<F>>>`
  plus `Arc<AtomicBool>` flags pattern stays. Phase 4 adds a `color_dirty` flag
  alongside the existing `render_task_is_busy` and `redraw_required` flags.

### 8.2 Render trigger matrix

| Event                 | What runs                              | Quality                              |
| --------------------- | -------------------------------------- | ------------------------------------ |
| Pan / zoom / click    | Full pipeline (a)→(d)                  | `sampling_level` per regulator       |
| Color edit (Phase 4+) | `recolorize_only`: refresh LUTs + (d)  | Honors current cached-field upsample |
| Space pressed         | Full pipeline (a)→(d)                  | Forced to user JSON `sampling_level` |
| Idle (no interaction) | Adaptive regulator may trigger upgrade | `sampling_level` climbing → full     |

When both `color_dirty` and a view change are pending, the view change (full
pipeline) takes priority — it regenerates the field that a recolorize would
otherwise re-walk.

### 8.3 Adaptive regulator

Stays unchanged from today's
[src/core/render_quality_fsm.rs](../src/core/render_quality_fsm.rs). The
`user_interaction = true` signal continues to come from view changes. Whether to
also feed color edits into this signal is **deferred to Phase 7** — the
regulator self-tunes from observed compute time, so the choice doesn't change the
architecture, only the UX feel of "how aggressively does quality bounce back up
after the user stops dragging a slider."

### 8.4 Static-dispatch invariant

The renderer hot path is fully monomorphized over `F: Renderable`. The core
iteration helpers (`compute_raw_field`, `populate_histograms`,
`colorize_collapse_unified`) are generic over `K: FieldKernel` and instantiated
once per fractal at compile time. There is no `dyn Renderable` and no runtime
variant check on the per-(sub)pixel hot path. The only runtime dispatch is the
`match fractal_params { … }` (and Newton's inner `SystemType` match) at startup
that selects which concrete `F` to instantiate. Per-frame and per-(sub)pixel
allocations are zero; all buffers are owned by `RenderingPipeline` / `PixelGrid`
and reused in place.

---

## 9. Testing Strategy

**Bar:** strong unit tests on logical pieces; manual smoke testing on the GUI
itself. Snapshot or behavioral GUI tests are not required for this roadmap but
may be added later if a particular bug class becomes recurring.

### 9.1 What to unit-test (mandatory)

- `colorize_cell` correctness on the unified `ColorPalette` (already covered in
  [color_map.rs](../src/core/color_map.rs) tests): all-`None` field →
  background; single-keyframe constant-color maps; boundary keyframe values;
  multi-color-map routing (`color_map_index = k` colorizes through
  `color_maps[k]`); empty `color_maps` rejected at deserialization.
- Core iteration helpers in
  [field_iteration.rs](../src/core/field_iteration.rs) against synthetic
  `FieldKernel` impls (already covered): positive / zero / negative
  `sampling_level` traversal; histogram routing; subpixel-to-real-space mapping.
- **New for Phase 4:**
  - Fraction renormalization: edit one fraction in a 4-keyframe color map,
    assert the others scale proportionally and positions match. Edge cases:
    edit to ε, to 1−ε, to 0 (clamped), to 1.0 (clamped).
  - Keyframe insertion: `+` between two keyframes yields the expected midpoint
    position and interpolated color.
  - Keyframe deletion: removing the middle of three preserves the 0.0/1.0
    anchors and drops the middle one.
  - `recolorize_only` equivalence: after a full `render`, calling
    `recolorize_only` with the same palette produces a byte-identical output
    image (the recolorize fast path is a no-op when keyframes are unchanged).

### 9.2 What to manually smoke-test (mandatory each phase)

Per the per-phase PR checklist (§11.3). Same matrix as today: Windows native,
WSL2/XWayland, native Linux, macOS.

### 9.3 What to leave for later

- egui snapshot tests on the editor panel rendering (would require
  `egui_kittest` or similar dev-dep).
- Synthetic-input behavioral tests (e.g. "click keyframe 2, press Delete, assert
  N−1 keyframes").
- A criterion benchmark for `colorize_collapse_unified` — add it only if live
  recolorize latency becomes a recurring concern (Phase 7).

---

## 10. Risks & De-risk

**Editor mutation corrupts the palette shape**

- **Phase:** 4
- **Mitigation:** the editor never adds/removes whole color maps; keyframe
  insert/delete preserve the 0.0/1.0 anchors; fraction edits renormalize and
  clamp to `[ε, 1]`. Unit-tested per §9.1.

**Live recolorize too slow at high resolution**

- **Phase:** 4, 7
- **Mitigation:** the recolorize path skips compute/histogram/CDF entirely
  (refresh LUTs + re-walk). Benchmark over a representative populated field at
  1920×1080 / 2K if it feels laggy; debounce or downsample-while-dragging falls
  to Phase 7.

**Saved JSON isn't reloadable**

- **Phase:** 5
- **Mitigation:** the snapshot must serialize the tagged `FractalParams`, not
  the bare inner params (see the §5 Phase-5 design wrinkle). Verify by reloading
  the saved file via `explore <saved.json>` and confirming the preview matches
  the saved PNG (§7.3).

**Save flow races with an in-flight interactive render**

- **Phase:** 5
- **Mitigation:** the save state machine locks input and forces a fresh
  full-quality render before writing; it does not snapshot a degraded buffer.

**Tab count drifts from `color_maps.len()`**

- **Phase:** 4
- **Mitigation:** the tab strip is a pure view of `color_maps.iter().enumerate()`;
  no separately stored count.

**Editor state desync after tab switch**

- **Phase:** 4
- **Mitigation:** selection state resets on tab change (§6.2).

A wrong-shape color map cannot reach the renderer at runtime: there is one
`ColorPalette` type, validated at JSON deserialization (empty `color_maps` is
rejected there and again at cache construction).

---

## 11. Working Practices

### 11.1 CI checks (per [CLAUDE.md](../CLAUDE.md))

Before every commit:

```bash
cargo fmt                    # CI checks with --check
cargo clippy -- -D warnings  # zero warnings
cargo test
cargo bench --no-run         # benchmarks must compile
npm run fmt:check            # Prettier formatting for JSON and Markdown
```

Pre-commit hooks in [.claude/settings.json](../.claude/settings.json) enforce
these automatically when committing via Claude Code. Run `npm run fmt` to
auto-format JSON/Markdown.

### 11.2 Branch / commit conventions

- Branches: `feature/description`, `fix/description`, `perf/description`,
  `refactor/description`.
- Commits: conventional (`feat:`, `fix:`, `perf:`, `refactor:`, `test:`,
  `docs:`, `chore:`) or imperative short titles.
- One logical change per commit.
- Include attribution for AI-assisted commits.
- Never push or open PRs without explicit user confirmation.

### 11.3 Per-phase PR checklist

- [ ] All CI green locally (fmt, clippy, test, bench --no-run, `npm run fmt:check`).
- [ ] Unit tests added for new pure-logic pieces (per §9.1).
- [ ] Pixel-hash regression tests pass unchanged (Phase 4 changes no
      render-math, so all hashes hold; investigate any drift as a bug).
- [ ] Manual smoke-test on various platforms (WSL, Windows, macOS, Linux).
- [ ] If a hot path changed: `cargo bench` comparison before/after.
- [ ] If a saved file's schema changed (Phase 5): a saved JSON re-loads via
      `explore` and reproduces the captured state.
- [ ] Doc updates: this roadmap reflects what was actually shipped (move
      in-progress phases to "done" or amend if scope shifted).

---

## 12. Open Questions for the Implementer

These do not block any phase but should be decided as the relevant phase lands.

1. ~~**Drop the `QuadraticMap<T>` wrapper.**~~ **Resolved in Phase 3.**
   Mandelbrot and Julia now implement `Renderable` / `FieldKernel` /
   `SpeedOptimizer` via blanket impls over `T: QuadraticMapParams`; the wrapper
   is gone.
2. **Active tab on switch.** When the active color-map tab changes, reset
   keyframe selection. Recommended; specified in §6.2.
3. **Reuse of `paint_gradient_bar`.** The current
   [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs) already
   implements an artifact-free gradient bar. Lift it into
   `src/core/interactive/editor.rs` in Phase 4 rather than rewriting; delete the
   demo module in Phase 6.
4. **Color edits → adaptive regulator?** Whether to feed color edits into
   `user_interaction = true`. Defer to Phase 7 measurement.
5. **`recolorize_only` granularity.** Whether to add a dedicated
   `refresh_luts_and_background` (skipping the CDF rebuild) or just re-call
   `refresh_after_compute_pass` (CDFs rebuild identically from the retained
   histograms). Benchmark before optimizing; the simpler reuse is the default.
6. **DDP basin coloring richness.** DDP currently collapses all non-zero basins
   into one constant-color map. Future work could expose per-basin colors by
   emitting the basin index as the color-map index in `evaluate` and shipping
   per-basin color maps in the JSON. Out of scope for this roadmap.

---

## 13. Quick Start for a New Agent

1. Read this doc end-to-end.
2. Read [src/core/user_interface.rs](../src/core/user_interface.rs) — the
   current explore app you're lifting into `src/core/interactive/`.
3. Read [src/core/render_window.rs](../src/core/render_window.rs) to understand
   `PixelGrid`, the `RenderWindow` trait, and the background-render pattern
   (`render_task_is_busy` / `redraw_required` flags).
4. Read [src/core/render_pipeline.rs](../src/core/render_pipeline.rs) and
   [src/core/field_iteration.rs](../src/core/field_iteration.rs) to understand
   the four-step pipeline you'll add `recolorize_only` to.
5. Read [src/core/color_map.rs](../src/core/color_map.rs) for the `ColorPalette`
   / `ColorMap` / `ColorPaletteCache` types the editor mutates, and
   [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs) for the
   `paint_gradient_bar` technique to lift.
6. Skim [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs),
   [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs),
   and [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs) to see
   how each embeds `ColorPalette` and implements `Renderable` / `FieldKernel`.
7. Confirm `cargo test` passes on `main`. Start Phase 4 (§5). Make a small first
   commit (the module reorg, no behavior change) to keep the diff reviewable.
8. Re-read §5 for that phase's detail, §6/§7 for the widget and save specs, and
   §10 for phase-specific risks.

Good luck.
