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

| Mode         | File                                                                                           | Status                                                       |
| ------------ | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Explore      | [src/core/user_interface.rs](../src/core/user_interface.rs)                                    | `eframe::App`; preview-only; full pan/zoom/save behavior     |
| Color editor | [src/core/color_map_editor_ui.rs](../src/core/color_map_editor_ui.rs)                          | `eframe::App`; demo widgets only — does not affect renderer  |

Both apps share [src/core/eframe_support.rs](../src/core/eframe_support.rs) for
`wgpu` setup, but everything else is duplicated.

### CLI

[src/cli/args.rs](../src/cli/args.rs) defines three subcommands: `Render`,
`Explore`, `ColorSwatch`. The `Explore` subcommand dispatches in
[src/cli/explore.rs](../src/cli/explore.rs) on the `FractalParams` variant:
Mandelbrot/Julia/DDP go through generic `PixelGrid<F>`; Newton has its own
explore path in [src/fractals/newtons_method.rs:461](../src/fractals/newtons_method.rs#L461).
BarnsleyFern and Serpinsky panic with "ERROR: Parameter type does not yet
implement RenderWindow" — they are intentionally out of scope for `explore`.

### Color-map representations (the central problem this roadmap addresses)

The three explorable fractal families today use structurally different color
representations:

| Fractal              | Representation                                                                              |
| -------------------- | ------------------------------------------------------------------------------------------- |
| Mandelbrot, Julia    | `ColorMapParams` = 1 gradient + 1 flat background ([src/fractals/quadratic_map.rs:19](../src/fractals/quadratic_map.rs#L19)) |
| Driven-damped pendulum | No color params at all — hard-coded white/black in `render_point` ([src/fractals/driven_damped_pendulum.rs:38-44](../src/fractals/driven_damped_pendulum.rs#L38-L44)) |
| Newton's method      | `CommonParams` with `boundary_set_color_rgb` + `cyclic_attractor_color_rgb` + `ColorMapSpec` enum (FullColorSpec or GrayscaleSpec) ([src/fractals/newtons_method.rs:204-266](../src/fractals/newtons_method.rs#L204-L266)) |

Unifying these is Phase 1.

---

## 3. Phase Roadmap Summary

| Phase | Title                          | Blast radius                                          |
| ----- | ------------------------------ | ----------------------------------------------------- |
| 1     | Color-map data unification     | All fractal params + every example/test JSON file     |
| 2     | Compute / color split          | `Renderable` trait + each fractal's render pipeline   |
| 3     | Unified `FractalApp` shell     | New `src/core/interactive/` module; preview only      |
| 4     | Color editor panel             | Editor widget + layout wiring                         |
| 5     | CLI + cleanup + Space-as-save  | Delete legacy modules; extend snapshot behavior       |
| 6     | Live color sync                | `PixelGrid` extended with re-colorize-only path       |
| 7     | Polish                         | Contents TBD post-Phase-6 measurement                 |

Phases 1 and 2 are independent pre-work — either order works, but Phase 1 is
recommended first so Phase 2's `colorize` function can target `UnifiedColorMap`
directly rather than per-fractal types it would later have to migrate.

Phases 3 → 6 are sequential. Phase 7 is opportunistic.

Each phase is a self-contained PR, bisectable, independently revertible.

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

Two paired enums underpin the entire roadmap. The pairing is structural: each
`UnifiedColorMap` variant has exactly one matching `CachedField` variant, and
every fractal type fixes the variant pair it uses.

### 5.1 `UnifiedColorMap`

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum UnifiedColorMap {
    /// Two flat colors. Every pixel belongs to one of two classes; no
    /// gradient information. Used by the driven-damped pendulum.
    ForegroundBackground {
        foreground: [u8; 3],
        background: [u8; 3],
    },

    /// One flat background plus one gradient. The flat color is for points
    /// that don't produce a graded output (e.g. inside the Mandelbrot set);
    /// the gradient is sampled by a normalized scalar (smooth escape count).
    /// Used by Mandelbrot and Julia.
    BackgroundWithColorMap {
        background: [u8; 3],
        color_map: Vec<ColorMapKeyFrame>,
    },

    /// One flat color plus one gradient per attractor. The flat covers
    /// "did-not-converge" points; each attractor gets its own gradient
    /// sampled by a smooth iteration count. Used by Newton's method.
    MultiColorMap {
        cyclic_attractor: [u8; 3],
        color_maps: Vec<Vec<ColorMapKeyFrame>>,
    },
}
```

Variant names are descriptive (what the structure is), not fractal-named.
Newton's `boundary_set_color_rgb` is intentionally absent: it is dead code in
the renderer ([src/fractals/newtons_method.rs:416-428](../src/fractals/newtons_method.rs#L416-L428)
never reads it) and exists today only to define the "in-set" endpoint of the
`GrayscaleSpec` shorthand, which Phase 1 drops.

### 5.2 `CachedField`

```rust
pub enum CachedField {
    /// Per-pixel basin index. Used by the driven-damped pendulum.
    BinaryBasin(Vec<Vec<Option<i32>>>),

    /// Per-pixel smooth escape count. Used by Mandelbrot and Julia.
    SmoothScalar(Vec<Vec<Option<f32>>>),

    /// Per-pixel (smooth iteration count, attractor index). Used by
    /// Newton's method.
    SmoothScalarWithIndex(Vec<Vec<Option<(f32, usize)>>>),
}
```

### 5.3 Pairing table

| `UnifiedColorMap` variant | `CachedField` variant      | Fractal                  | Colorization rule                                                                  |
| ------------------------- | -------------------------- | ------------------------ | ---------------------------------------------------------------------------------- |
| `ForegroundBackground`    | `BinaryBasin`              | DDP                      | `Some(0)` → `foreground`; `Some(_)` or `None` → `background`                       |
| `BackgroundWithColorMap`  | `SmoothScalar`             | Mandelbrot, Julia        | `None` → `background`; `Some(f)` → `color_map.lookup(f)`                           |
| `MultiColorMap`           | `SmoothScalarWithIndex`    | Newton's method          | `None` → `cyclic_attractor`; `Some((f, k))` → `color_maps[k].lookup(f)`            |

The free function `colorize(field: &CachedField, map: &UnifiedColorMap) ->
ColorImage` matches on the `(field, map)` tuple. Mismatched variants
(`BinaryBasin` × `BackgroundWithColorMap`, etc.) panic with a clear "wiring
bug" message — by construction this case never arises from valid params files,
since each fractal's `Renderable` impl returns the correct `CachedField`
variant and embeds the matching `UnifiedColorMap` variant.

### 5.4 Why the typed enum (vs a `Vec<LabeledGradient>` flat list)

Considered earlier and rejected:

- A flat `Vec<LabeledGradient>` would require positional conventions
  ("entry 0 is always background, entries 1..N are gradients") that are easy
  to drift between editor and renderer, and the editor would still need a
  "is this a flat color or a gradient?" discriminator per entry, plus a
  per-fractal label lookup table.
- The typed enum gives compiler-enforced exhaustiveness, role-as-data
  (no positional contract), no round-trip conversion at the editor/renderer
  boundary, and small editor dispatch (one match arm per variant, ~25 LoC,
  with shared `show_swatch` / `show_gradient_editor` widget helpers).

---

## 6. Phase Detail

### Phase 1 — Color-map data unification

**Goal:** every fractal type embeds `UnifiedColorMap` directly in its params
struct. JSON schema migrates accordingly. No GUI work.

**Files touched:**

- [src/core/color_map.rs](../src/core/color_map.rs) — define `UnifiedColorMap`.
- [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs) — replace
  `ColorMapParams.keyframes` and `background_color_rgb` with `color:
  UnifiedColorMap` (constrained to `BackgroundWithColorMap`). The other
  `ColorMapParams` fields (`lookup_table_count`, `histogram_bin_count`,
  `histogram_sample_count`) are not color data and stay on `QuadraticMapParams`
  directly (or move into a sibling struct — implementer's choice).
- [src/fractals/mandelbrot.rs](../src/fractals/mandelbrot.rs),
  [src/fractals/julia.rs](../src/fractals/julia.rs) — automatic via the
  `QuadraticMapParams` trait change.
- [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs)
  — add `color: UnifiedColorMap` field with
  `#[serde(default = "ddp_default_color")]`. Default is
  `ForegroundBackground { foreground: [255,255,255], background: [0,0,0] }`,
  matching the previously hard-coded values. Replace the literal
  `Rgb([255,255,255])` / `Rgb([0,0,0])` in `render_point` with reads from the
  field.
- [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs) —
  replace `boundary_set_color_rgb`, `cyclic_attractor_color_rgb`, and
  `color_map_spec` in `CommonParams` with a single `color: UnifiedColorMap`
  field (constrained to `MultiColorMap`). **Drop `GrayscaleSpec` and
  `GrayscaleKeyframeSpec` entirely**, including the `to_color_map_vec`
  expansion logic. The `ColorMapSpec` enum can be removed; if its other
  variant `FullColorSpec` is kept around as an alias type, that's fine, but
  the simpler path is to delete it too.
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

**Variant slack:** each fractal's params struct holds `color: UnifiedColorMap`
typed as the full enum, not a refinement to one variant. Construction-time
validation (or a `match` in the renderer that panics on the wrong variant)
catches misuse. The slack is acceptable in exchange for not needing per-fractal
narrow types.

### Phase 2 — Compute / color split

**Goal:** factor `Renderable` so the per-pixel scalar computation is separated
from colorization. No observable behavior change; pixel hashes unchanged.

**Files touched:**

- [src/core/image_utils.rs](../src/core/image_utils.rs) — extend `Renderable`
  trait. Add `CachedField` enum. Add `colorize(field, map)` and
  `colorize_into(field, map, &mut buffer)` free functions.
- [src/fractals/quadratic_map.rs](../src/fractals/quadratic_map.rs),
  [src/fractals/driven_damped_pendulum.rs](../src/fractals/driven_damped_pendulum.rs),
  [src/fractals/newtons_method.rs](../src/fractals/newtons_method.rs) —
  implement `compute_field()`; default `render_to_buffer` to
  `compute_field()` then `colorize_into(...)`. Where today's renderer does
  histogram/CDF setup that depends on the field but not the keyframes
  (e.g. [src/fractals/quadratic_map.rs:225](../src/fractals/quadratic_map.rs#L225)),
  that work happens at the `compute_field` boundary so the per-fractal
  prep cost is paid once per compute, not once per re-colorize.

**Sketch:**

```rust
pub trait Renderable: Sync + Send + SpeedOptimizer {
    type Params: Serialize + Debug;

    fn compute_field(&self) -> CachedField;
    fn color_map(&self) -> &UnifiedColorMap;
    // color_map_mut added in Phase 6, not here.

    // Existing methods unchanged: image_specification, render_options,
    // set_image_specification, write_diagnostics, params, render_point.

    // Default impl rewires through compute + colorize:
    fn render_to_buffer(&self, buffer: &mut Vec<Vec<Rgb<u8>>>) {
        let field = self.compute_field();
        colorize_into(&field, self.color_map(), buffer);
    }
}
```

**Unit tests added:** `colorize` correctness for each `(CachedField,
UnifiedColorMap)` pairing, including edge cases (empty `color_maps` for
`MultiColorMap`, all-`None` field for each variant, single-keyframe gradients).

**Verification:** pixel-hash regression tests must pass unchanged. `cargo
bench --no-run` to confirm benchmarks still compile; `cargo bench` on the
quadratic-map histogram benchmark to confirm no regression on the hot path.

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

**Verification:** manual smoke-test all four fractal types on Windows native
+ WSL.

### Phase 4 — Color editor panel

**Goal:** add the right-side color editor panel to `FractalApp`. Editor
displays the loaded `UnifiedColorMap` and allows local mutation of a
**cached copy**. The fractal preview is not affected by edits — that's
Phase 6.

**Files touched:**

- `src/core/interactive/editor.rs` — new; per-variant editor widgets,
  shared helpers (`show_swatch`, `show_gradient_editor`), tab strip for
  `MultiColorMap`.
- `src/core/interactive/app.rs` — extend layout: `SidePanel::right` for the
  editor, `CentralPanel` for the preview. `FractalApp` gains
  `editor_color_map: UnifiedColorMap` and editor selection state (selected
  keyframe index, active Newton tab).

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
│                                             │ [color picker]       │
└─────────────────────────────────────────────┴──────────────────────┘
```

Detailed widget spec is in §7.

**Local-cache lifecycle:** `editor_color_map` is initialized at startup as a
clone of the renderer's `UnifiedColorMap`. All editor widgets mutate only this
cache. The renderer continues to use its own (immutable, in this phase) map.
Edits do not survive window close (no save-back to disk; Space-as-save in
Phase 5 captures the cache to a fresh timestamped JSON).

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

**Approach:** the `CachedField` produced by `compute_field` does not depend
on the color map, so we cache it after each compute and re-colorize from it
on every color edit. Re-colorize is pure pixel-level work (no fractal math)
and is fast enough to run on demand.

**Files touched:**

- [src/core/image_utils.rs](../src/core/image_utils.rs) — add
  `color_map_mut(&mut self) -> &mut UnifiedColorMap` to `Renderable`.
- [src/core/render_window.rs](../src/core/render_window.rs) — extend
  `PixelGrid` to:
  - Cache the most recent `CachedField` after each compute (alongside the
    existing `display_buffer`).
  - Add an `Arc<AtomicBool>` `color_dirty` flag.
  - In `update()`, if `color_dirty` is set and no compute is in flight,
    spawn a background task that just re-colorizes the cached field into
    `display_buffer` (skipping `compute_field`).
- `src/core/interactive/app.rs` — wire editor edits: when the editor mutates
  a keyframe / fraction / flat color, write the change into
  `renderer.color_map_mut()`, set `color_dirty`, call `ctx.request_repaint()`.
- `src/core/interactive/editor.rs` — return an "edited" boolean from each
  editor function so the app knows when to set `color_dirty`.

**Editor cache transition:** the separate `editor_color_map` from Phase 4
becomes redundant. The editor now mutates `renderer.color_map_mut()` directly.
The app retains only editor *selection* state (selected keyframe index, active
Newton tab) — the data lives on the renderer.

**Adaptive quality regulator interaction:** color edits trigger only
re-colorize, not re-compute, so the regulator's compute-quality scaling is not
mechanically engaged by them. Whether color edits should also feed the
`user_interaction = true` signal (so the regulator stays in "interactive
mode" and defers expensive idle-time recomputes) is a UX-feel decision to
make once Phase 6 is functional. The regulator self-tunes from observed
compute time, so neither choice is structurally wrong.

**Verification:** manual interactive testing — drag fraction sliders, click
keyframe colors, verify the preview updates within a frame or two. Benchmark
`colorize` over a representative `SmoothScalar` field at 1920×1080 and 4K to
confirm it stays under one frame at 60Hz; if not, Phase 7 must include
optimization or downscaling for the live path.

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
  showing the *fraction* of the gradient occupied by that segment (the
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
  *other* fractions are scaled proportionally so the sum stays 1.0; the
  keyframe positions are recomputed from the resulting fractions. Each
  fraction is clamped to `[ε, 1.0]` (with `ε ≈ 0.005`) to prevent any
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

| Key                | Behavior                                                                         |
| ------------------ | -------------------------------------------------------------------------------- |
| Arrow keys         | Pan view (existing).                                                             |
| W / S              | Zoom in / out (existing).                                                        |
| A / D (with no W/S)| Fast zoom in / out (existing).                                                   |
| R                  | Reset to initial view (existing).                                                |
| Mouse left-click   | Recenter view on clicked point in the fractal preview (existing).                |
| Space              | Save snapshot — see §8.                                                          |
| Q                  | Exit application.                                                                |
| Ctrl+C             | Exit application (terminal default).                                             |
| Esc                | Clear keyframe selection. **No-op when no keyframe is selected.** Does not exit. |
| Delete             | Remove selected keyframe (no-op for first/last).                                 |

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
full-quality render and locks input until complete.

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
   pushed it to.
3. **Sync color map.** Push the editor's current `UnifiedColorMap` (which in
   Phase 6 *is* the renderer's map; in Phase 5 was a separate cache) into the
   renderer's params for serialization.
4. **Render to GUI.** Background thread runs `compute_field` (full quality)
   followed by `colorize` (with synced map) and swaps the result into the
   preview texture. The save flow blocks (overlay still up) until the render
   is complete.
5. **Save params to disk.** Serialize the now-synced `FractalParams`
   (including embedded `UnifiedColorMap` and the current view-control's
   `image_specification`) to `<prefix>_<datetime>.json`. The
   filename pattern matches today's
   [src/core/render_window.rs:255-261](../src/core/render_window.rs#L255-L261).
6. **Save image to disk.** Write the just-rendered buffer to
   `<prefix>_<datetime>.png`. Pixels match what's on screen.
7. **Unlock.** Clear `save_in_progress`; remove overlay; resume input.

### 8.3 Restorability invariant

Calling `cargo run -- explore <saved.json>` on the file produced by step 5
must restore the GUI to *exactly* the state it was in when Space was pressed:
the same view bounds, the same color map (including any edits), the same
render quality, the same fractal type. The pixel hash of the rendered preview
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

| Event                          | What runs                                  | Quality           |
| ------------------------------ | ------------------------------------------ | ----------------- |
| Pan / zoom / click             | `compute_field` + `colorize`               | Adaptive          |
| Color edit (Phase 6+)          | `colorize` only (uses cached field)        | Full              |
| Space pressed                  | `compute_field` + `colorize`               | Forced to full    |
| Idle (no interaction)          | Adaptive regulator may trigger upgrade     | Climbing → full   |

### 9.3 Adaptive regulator

Stays unchanged from today's
[src/core/render_quality_fsm.rs](../src/core/render_quality_fsm.rs). The
`user_interaction = true` signal continues to come from view changes. Whether
to also feed color edits into this signal is **deferred to Phase 7** — the
regulator self-tunes from observed compute time, so the choice doesn't change
the architecture, only the UX feel of "how aggressively does quality bounce
back up after the user stops dragging a slider."

### 9.4 BarnsleyFern and Serpinsky

Continue to panic in `cli::explore::explore_fractal` with "Parameter type does
not yet implement RenderWindow." Out of scope for this entire roadmap. Their
params structs are not migrated to `UnifiedColorMap` (Phase 1) and they do not
implement `compute_field` (Phase 2).

---

## 10. Testing Strategy

**Bar:** strong unit tests on logical pieces; manual smoke testing on the GUI
itself. Snapshot or behavioral GUI tests are not required for this roadmap
but may be added later if a particular bug class becomes recurring.

### 10.1 What to unit-test (mandatory)

- `colorize(field, map)` correctness for every `(CachedField, UnifiedColorMap)`
  pairing, including:
  - All-`None` field for each variant.
  - Single-keyframe gradients.
  - Boundary keyframe values (0.0 and 1.0).
  - `MultiColorMap` with empty `color_maps` (must error or panic with a
    clear message — not silently produce garbage).
- Fraction renormalization: edit one fraction in a 4-keyframe gradient,
  assert the others scale proportionally and the resulting positions match
  expectations. Edge cases: edit to ε, edit to 1−ε, edit to 0 (clamped),
  edit to 1.0 (clamped).
- Keyframe insertion: `+` between two existing keyframes produces the
  expected midpoint position and the expected interpolated color.
- Keyframe deletion: removing the second keyframe in a 3-keyframe gradient
  preserves positions 0.0 and 1.0 of the anchors and removes the middle one.
- `UnifiedColorMap` serde round-trips for every variant.
- `CachedField` construction for each fractal: a small synthetic
  `ImageSpecification` produces the expected variant with the expected size
  and the expected entries at known points.
- DDP `#[serde(default)]` shim: an existing pre-Phase-1 DDP JSON
  (re-created in a test fixture) still parses and produces the
  hard-coded white/black colors.

### 10.2 What to manually smoke-test (mandatory each phase)

Per the per-phase PR checklist (§12). Same matrix as today: Windows native,
WSL2/XWayland, native Linux if available.

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

| Risk                                                                | Phase | Mitigation                                                                                                |
| ------------------------------------------------------------------- | ----- | --------------------------------------------------------------------------------------------------------- |
| JSON migration misses a file                                        | 1     | `tests/example_parameter_validation_tests.rs` glob covers all JSONs; CI catches missed migrations.        |
| Schema migration changes pixel hashes                               | 1     | Pixel-hash regression tests gate the PR; if hashes change, the migration changed semantics — bug.         |
| C0b refactor breaks pixel hashes                                    | 2     | Pure refactor; pixel-hash tests are the gate.                                                             |
| `colorize` too slow at 4K to be live                                | 6     | Benchmark over representative `SmoothScalar` field at 4K early in Phase 6. Falls back to Phase 7 work.    |
| Newton tab count drifts from `color_maps.len()`                     | 4, 6  | Tab strip is a pure view of `color_maps.iter().enumerate()`; no separately stored count.                  |
| Variant mismatch (e.g. Mandelbrot params with `MultiColorMap`)      | 1     | Renderer panics with clear message at construction. Authoring-time error, never reachable from valid use. |
| Editor state desync after `MultiColorMap` tab switch                | 4, 6  | Selection state resets on tab change (specified in §7.2).                                                 |
| Adaptive regulator behaves badly during color editing               | 6, 7  | Regulator self-tunes; if behavior is wrong, Phase 7 adjusts whether color edits feed `user_interaction`.  |

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
- Include `Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>` for
  AI-assisted commits.
- Never push or open PRs without explicit user confirmation.

### 12.3 Per-phase PR checklist

- [ ] All CI green locally (fmt, clippy, test, bench --no-run).
- [ ] Unit tests added for new pure-logic pieces (per §10.1).
- [ ] Pixel-hash regression tests pass unchanged where applicable
      (Phases 1, 2 especially).
- [ ] Manual smoke-test on Windows native.
- [ ] Manual smoke-test on WSL.
- [ ] Manual smoke-test on native Linux (if available).
- [ ] If a hot path changed: `cargo bench` comparison before/after.
- [ ] If JSON schema changed: every example JSON re-loads and produces the
      same image hash (or a documented and intended pixel difference).
- [ ] Doc updates: this roadmap reflects what was actually shipped (move
      in-progress phases to "done" or amend if scope shifted).

---

## 13. Open Questions for the Implementer

These do not block any phase but should be decided as the relevant phase
lands.

1. **Variant slack at the type level.** Each fractal stores
   `color: UnifiedColorMap` as the full enum but only ever uses one variant.
   If this becomes painful in practice (e.g. lots of `match` arms doing
   `_ => panic!`), consider introducing typed wrappers at the params level
   (`BackgroundWithColorMap` as its own struct that `Into<UnifiedColorMap>`).
   Default: live with the slack; revisit if it bites.
2. **Active Newton tab on switch.** When the active tab changes, reset
   keyframe selection. Recommended.
3. **Reuse of `paint_gradient_bar`.** The current
   [src/core/color_map_editor_ui.rs:215-241](../src/core/color_map_editor_ui.rs#L215-L241)
   already implements an artifact-free gradient bar. Keep it; lift into
   `src/core/interactive/editor.rs` rather than rewriting.
4. **Color edits → adaptive regulator?** Whether to feed color edits into
   `user_interaction = true`. Defer to Phase 7 measurement.
5. **DDP basin coloring richness.** Today DDP collapses all non-zero basins
   into one "background" bucket. Future work could expose per-basin colors,
   which would require extending `UnifiedColorMap` with a richer DDP variant
   (or adopting `MultiColorMap` for DDP). Out of scope for this roadmap.

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
