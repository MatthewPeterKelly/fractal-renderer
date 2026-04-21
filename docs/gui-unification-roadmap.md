# GUI Unification Roadmap

This document describes the multi-phase plan to consolidate the project onto a
single cross-platform GUI architecture built on `eframe`/`egui`, on Rust 2024
edition, while eliminating legacy `pixels` + hand-rolled `winit` code paths.

**Audience:** a new agent or contributor picking up the GUI work. This doc is
self-contained — no prior conversation context is needed.

---

## 1. End State Vision

The binary ships with exactly two modes:

1. **Headless render mode** (CLI: `fractal-renderer render ...`) — unchanged
   from today. Writes images to disk based on a params JSON file. No GUI.
2. **Interactive mode** (CLI: `fractal-renderer interactive ...` or similar) —
   a single unified GUI window that combines:
   - Fractal preview (pan/zoom/click, as `explore` mode does today)
   - Color map editor (gradient bar, keyframe editing, as the color editor does
     today)
   - Parameter inspector (view/edit fractal params live)

The interactive mode is built entirely on `eframe` (egui's official framework),
with a **background render thread** feeding a `TextureHandle` for live updates.

**What disappears:**

- `pixels` crate and everything that depends on it
- Direct `winit` usage (eframe owns the event loop)
- The `cli/explore.rs` + `core/user_interface.rs` hand-rolled event loop

---

## 2. Current State (as of branch landing this roadmap)

### What exists today

Two independent GUI code paths share no infrastructure:

| Mode         | File                                           | Framework                                | Dependencies           |
| ------------ | ---------------------------------------------- | ---------------------------------------- | ---------------------- |
| Explore mode | `src/core/user_interface.rs` (~365 lines)      | hand-rolled `winit 0.28` + `pixels 0.13` | no egui                |
| Color editor | `src/core/color_map_editor_ui.rs` (~310 lines) | `eframe 0.22` (`egui 0.22`)              | no direct winit/pixels |

### Recent work landed on the `color-gui/...` branch

1. Migrated color editor from hand-rolled `pixels` + `egui-wgpu` + `egui-winit`
   to `eframe 0.22`. Shrank the file ~185 lines.
2. Set `panel_fill = BLACK`, `bg_stroke = NONE`, `show_separator_line(false)`
   to eliminate egui's default separator lines.
3. Overrode panel frames with `Frame::none().fill(BLACK)` to eliminate
   sub-pixel gap artifacts at panel boundaries.
4. Replaced `SidePanel::exact_width` with `default_width` + `width_range` so
   the editor pane is actually resizable by dragging.
5. Replaced the gradient bar's `line_segment` loop with `rect_filled`
   (contiguous rectangles) to eliminate 1-logical-pixel stroke artifacts at
   fractional DPI.
6. Added defensive `ctx.request_repaint()` at the end of `App::update()` to
   self-correct from silently-dropped resize events on WSL/XWayland.

### Known bugs that remain (platform-level, not fixable on eframe 0.22)

| Bug                                              | Platform             | Cause                                                                                 | Fix                              |
| ------------------------------------------------ | -------------------- | ------------------------------------------------------------------------------------- | -------------------------------- |
| Window can't be dragged between monitors         | Windows (mixed DPI)  | `winit 0.28` `WM_DPICHANGED` feedback loop                                            | winit 0.29+ (via eframe 0.24+)   |
| Resize events silently dropped at fractional DPI | WSL/XWayland         | `winit 0.28` bug on Wayland scale changes                                             | winit 0.29.9+ (via eframe 0.24+) |
| No resize handles                                | WSL (some X servers) | X11 forwarding with no window manager — eframe already requests decorations correctly | environment-level (run a WM)     |

---

## 3. Hard Constraints

These are non-obvious gotchas discovered during the color editor migration.
Any future work must account for them.

### 3.1 The `wgpu_core` linker conflict

`wgpu_core` exports `#[no_mangle]` C symbols. Two versions of `wgpu_core` in
the same binary → **linker error** (duplicate symbols), not just a warning.

This means: **all crates that transitively depend on wgpu must share a
version.** Currently:

- `pixels 0.13` → `wgpu 0.16`
- `eframe 0.22` → `wgpu 0.16` ✓ (coincidentally matches)
- `eframe 0.24, 0.25, 0.26, 0.27` → `wgpu 0.19`
- `eframe 0.28` → `wgpu 0.20`
- `eframe 0.29+` → `wgpu 0.20+`

- `pixels 0.13` → `wgpu 0.16`
- `pixels 0.14` → `wgpu 0.17`
- `pixels 0.15` → `wgpu 0.19`
- `pixels 0.16` → `wgpu 0.29`

**Implication:** as long as the explore mode uses `pixels`, it constrains
`eframe` to a compatible version. The only way to upgrade `eframe` past 0.22
without dep-tree conflicts is to **remove `pixels` first** (by porting explore
mode to eframe).

### 3.2 The `raw-window-handle` conflict

`pixels 0.14+` uses `raw-window-handle 0.6`. `winit 0.28` uses
`raw-window-handle 0.5`. These version numbers are mutually incompatible at
the trait level — you cannot pass a `&winit::Window` (RWH 0.5) to a
`pixels::Pixels::new` (RWH 0.6).

**Implication:** `pixels` and `winit` versions are coupled too. You can't mix
`pixels 0.14+` with `winit 0.28`.

### 3.3 Rust edition requirements

- `eframe 0.32+` requires Rust edition 2024.
- Current `Cargo.toml` declares `edition = "2018"`.
- Edition 2024 has stricter rules (disjoint closure captures, temporary
  lifetimes, new reserved keywords like `gen`). A clean codebase usually needs
  only a handful of fixes, but it's an unknown ripple risk.

**Implication:** jumping to the latest eframe (0.34) requires a Rust edition
bump as a prerequisite. eframe 0.27 is the highest version that still compiles
on edition 2018/2021.

### 3.4 eframe API shifts between 0.22 and latest

When upgrading, expect these mechanical changes:

| 0.22                                   | 0.24+                                                         | 0.28+                                                         |
| -------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------- |
| `initial_window_size: Some(vec2(w,h))` | `viewport: ViewportBuilder::default().with_inner_size([w,h])` | same                                                          |
| `frame.close()`                        | `ctx.send_viewport_cmd(ViewportCommand::Close)`               | same                                                          |
| `Box::new(\|cc\| Box::new(App))`       | same                                                          | `Box::new(\|cc\| Ok(Box::new(App)))`                          |
| `egui::Image::new(&tex, size)`         | `Image::new(&tex).fit_to_exact_size(size)`                    | same                                                          |
| `egui::style::Margin`                  | `egui::Margin`                                                | same                                                          |
| `DragValue::clamp_range`               | deprecated → `.range()`                                       | same                                                          |
| `App::update` method                   | same                                                          | same (but `App::ui` preferred in 0.34)                        |
| Linux needs features                   | `eframe = { features = ["wgpu"] }`                            | `eframe = { features = ["wgpu", "x11", "wayland"] }` (v0.30+) |

---

## 4. Cross-Platform Learnings

Distilled from cross-platform testing on Windows native, WSL2/XWayland, and
code-level investigation of winit/egui/pixels source.

### 4.1 Border/line artifacts at panel boundaries

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

See `src/core/color_map_editor_ui.rs` for the current implementation that
applies all three fixes.

### 4.2 Resize event drops on WSL/XWayland

**Symptom:** window appears not to resize at all, or content stops updating
when the user drags the window edge.

**Root cause:** `winit 0.28` on Wayland can drop resize events when the scale
factor changes. Fixed in `winit 0.29.9`.

**Mitigation on old winit:** `ctx.request_repaint()` at the end of `update()`
forces eframe to re-poll surface size every frame, self-correcting on the
next paint cycle. Cost is modest since egui still short-circuits painting
when nothing changed.

**Real fix:** upgrade eframe to 0.24+ (pulls in winit 0.29+).

### 4.3 Windows multi-monitor drag stuck

**Symptom:** dragging a window between monitors with different DPI scales
causes the window to stutter and snap back to the original monitor, or get
"stuck" at the boundary.

**Root cause:** `winit 0.28`'s `WM_DPICHANGED` handler repositions the window
during drag, which can trigger another `WM_DPICHANGED` event, creating a
feedback loop. The pixel-by-pixel nudge logic in
`winit/src/platform_impl/windows/event_loop.rs` lines ~2248-2259 is bounded
but still visible to the user.

**Mitigation:** none available on winit 0.28. Fixed in winit 0.29+.

### 4.4 No resize handles on WSL

**Symptom:** the window has no visible resize borders; can't drag edges.

**Root cause:** environmental, not a code bug. eframe correctly passes
`decorated(true)` and `resizable(true)` to winit. On X11, window decorations
(title bar, resize borders) are provided by the **window manager** — WSL's X
server may not provide one.

**Mitigation:** run a window manager (`openbox`, `fluxbox`) alongside the X
server. Not fixable in-repo.

### 4.5 egui panel width locking

**Gotcha:** `SidePanel::exact_width(w)` does NOT just set the initial width —
it clamps `width_range` to `w..=w`, making the panel non-resizable even
though the resize drag handle still renders (just does nothing on drag).

**Fix pattern:** use `default_width(w).width_range(min..=max)` instead.

---

## 5. Roadmap

Four phases, each a self-contained PR. Each phase is bisectable and
independently revertible.

### Phase A: Port explore mode to eframe (on eframe 0.22)

**Goal:** unify the GUI framework. Remove `pixels` from the dep tree. Leave
everything else stable.

**Why eframe 0.22 specifically?** To minimize moving parts. The explore
mode's _migration_ is a big change; combining it with an eframe version bump
would make a bad PR. Do the version bump in Phase B, after the framework is
unified.

**Scope:**

- Rewrite `src/core/user_interface.rs` as an `eframe::App` implementation.
- Convert the CPU fractal buffer to `egui::ColorImage` → `TextureHandle`
  each frame the fractal changes.
- Replace `RawInputState` with `ctx.input()`.
- Replace `pixels.window_pos_to_pixel(...)` with math derived from
  `ui.min_rect()` and the fractal image aspect ratio.
- Delete `pixels` dep from `Cargo.toml`.
- Delete direct `winit` dep from `Cargo.toml` (eframe re-exports what's
  needed).
- Keep the background-ish render model: `PixelGrid` / `RenderWindow` in
  `src/core/render_window.rs` probably need small signature changes
  (e.g. accept `&mut ColorImage` or a raw `&mut [u8]` instead of
  `pixels.frame_mut()`).

**Risks:**

- `TextureHandle::set()` performance at 60+ Hz during pan/zoom. The explore
  mode re-renders continuously during interaction. Must benchmark early.
- Input semantics differ: `winit` keyboard events vs. `egui::Key`. Key
  mappings need a translation layer (or just use egui keys directly).
- Mouse-click-to-center behavior: needs to convert pointer position from
  egui `Pos2` in the fractal image's rect back to fractal space.

**De-risk first:**

- Spike: measure `TextureHandle::set(ColorImage)` + `egui::Image` render cost
  at 1920×1080, 100 Hz. If it's over 3ms/frame on target hardware, redesign
  (e.g. use `egui_wgpu::CallbackTrait` for a custom wgpu render pass that
  reuses an existing GPU texture).

**Expected outcome:**

- Explore mode works identically, but now cross-platform (inherits eframe's
  correct DPI/resize/multi-monitor handling — though still on winit 0.28's
  bugs until Phase B).
- Single Cargo.toml has just `eframe` + `egui` for GUI; no `pixels`, no
  direct `winit`.
- Both GUIs share the eframe App trait pattern.

### Phase B: Modernize the stack (edition 2024 + eframe 0.34)

**Prerequisite:** Phase A complete (no pixels/winit dep conflicts).

**Scope:**

- `Cargo.toml`: `edition = "2018"` → `edition = "2024"`.
- Bump `eframe = "0.22"` → `"0.34"` and `egui = "0.22"` → `"0.34"`.
- Fix the mechanical API shifts listed in §3.4.
- Fix any Rust 2024 edition issues surfaced by `cargo check`:
  - Disjoint closure captures may require explicit `&` or `&mut` in
    closures.
  - New reserved keywords (`gen`, `try`) — unlikely to affect this codebase.
  - Temporary lifetime rule tightening — may need rebinding in a few
    places.
- Verify CI is green on all checks (`cargo fmt`, `cargo clippy -D warnings`,
  `cargo test`, `cargo bench --no-run`).
- Update MSRV in CI config if relevant.

**Expected outcome:**

- Fully modern Rust + eframe stack.
- WSL resize-event drops and Windows multi-monitor-drag bugs fixed (via
  winit 0.29+).
- Latest egui widget set available for Phase C.

**Risks:**

- Edition 2024 ripple fixes could touch unrelated files (fractals modules,
  core modules). Allot time for discovery.
- egui 0.34 may have renamed or restructured widgets we use (especially
  `egui::color_picker`, `Image`, `Slider` step/clamp APIs). Each needs
  individual verification.

### Phase C: Merge into a unified `FractalApp`

**Prerequisite:** Phase B complete.

**Goal:** one `eframe::App` implementation serves both interactive
workflows.

**Design to settle (not yet decided):**

- **Layout:** likely a central fractal preview with docked panels:
  - Right panel: color map editor (expandable)
  - Left panel or top bar: fractal parameters (kind, iterations, bounds,
    etc.)
  - Bottom status: coordinates under cursor, render progress
- **State model:** a single `FractalAppState` struct holding:
  - Current `FractalParams` (the JSON type)
  - Latest rendered `Vec<Vec<Rgb<u8>>>` buffer + its resolution
  - `dirty: bool` flag per subsystem (view, params, colors)
  - `egui::TextureHandle` for the preview
- **Threading model:** background thread owns the render pipeline. UI thread
  sends commands (center, zoom, params-change) via channel; background
  thread posts completed buffers back via channel or shared
  `Arc<Mutex<Option<Buffer>>>`.
- **Redraw signaling:** `ctx.request_repaint()` from the UI side when input
  changes; background thread calls `ctx.request_repaint()` after posting a
  new buffer.

**Scope:**

- Move `explore` and `color_editor_ui` logic into a new
  `src/core/interactive/` module with submodules for panels.
- Wire up a single CLI subcommand (`interactive`) that takes a params JSON
  file and opens the unified GUI.
- Delete `src/cli/explore.rs` or keep it as a thin wrapper that calls the
  new interactive entry point with explore-focused defaults.
- Delete or quarantine the old `color_map_editor_ui.rs` once everything is
  in the new module.

### Phase D: Live color adjustment

**Prerequisite:** Phase C complete.

**Two sub-phases:**

#### D1. Color adjustments without re-rendering the fractal

Key insight: for escape-time fractals (Mandelbrot, Julia), the _histogram_
or _escape count grid_ doesn't depend on the color map — only the final
pixel coloring does. If we cache the escape-count grid, we can re-color
instantly on a palette change.

**Scope:**

- Refactor `Renderable` (or add a parallel trait) so the compute and color
  phases are separable:
  - `compute_escape_grid() -> EscapeGrid`
  - `color_grid(grid: &EscapeGrid, color_map: &ColorMap) -> ColorImage`
- Cache the `EscapeGrid` in the app state; re-color on any color edit.
- The gradient bar editing in the color panel → immediately re-colors the
  preview (target: <1 frame latency).

**What fractals this works for:** Mandelbrot, Julia, Newton's. Not
Barnsley fern (no escape count, it's an IFS), not pendulum (phase
portrait). Those need full re-render on color change — acceptable because
they're not the common color-editing case.

#### D2. Full dynamic parameter adjustment

Backend param changes (iteration cap, view bounds, fractal kind) trigger a
full re-render in the background thread. UI stays responsive with:

- Debounce param changes by ~100ms to batch rapid slider edits.
- Render at reduced resolution first (1/4 res), upgrade to full res when
  interaction idles for ~500ms.
- Cancellation: if a new render is requested while one is in flight, abort
  the in-flight render (check a `stop_flag: Arc<AtomicBool>` in the
  fractal hot loop).

---

## 6. Architecture & File Map

### Files central to the GUI (current state)

| File                              | Role                               | Phase that touches it                            |
| --------------------------------- | ---------------------------------- | ------------------------------------------------ |
| `src/core/color_map_editor_ui.rs` | Color editor `eframe::App`         | A (port reference), C (merge)                    |
| `src/core/user_interface.rs`      | Explore mode event loop            | A (rewrite), C (delete or absorb)                |
| `src/core/render_window.rs`       | `PixelGrid`/`RenderWindow` helpers | A (minor signature changes)                      |
| `src/cli/explore.rs`              | Thin CLI wrapper                   | A (no change), C (delete or wrap)                |
| `src/core/view_control.rs`        | Pan/zoom math                      | C (reuse)                                        |
| `src/core/color_map.rs`           | Color interpolation                | D1 (reuse for re-coloring)                       |
| `src/core/image_utils.rs`         | `Renderable` trait                 | D1 (split compute/color)                         |
| `examples/common/mod.rs`          | Calls `run_color_editor`           | A, C (update call)                               |
| `Cargo.toml`                      | Deps                               | A (remove pixels/winit), B (bump eframe/edition) |

### Key trait: `Renderable`

Defined in `src/core/image_utils.rs`. Currently:

```rust
pub trait Renderable {
    fn render_to_buffer(&self, buffer: &mut Vec<Vec<image::Rgb<u8>>>);
}
```

All fractal modules (`src/fractals/*.rs`) implement this. Phase D1 will
likely split this into compute + color phases.

---

## 7. Dependency Version Matrix

### Current (as of this doc's landing branch)

```toml
eframe = { version = "0.22", default-features = false, features = ["wgpu"] }
egui = "0.22"
pixels = "0.13"
winit = "0.28"
```

Rust edition: **2018**.

### Target after Phase A

```toml
eframe = { version = "0.22", default-features = false, features = ["wgpu"] }
egui = "0.22"
# pixels and winit removed entirely
```

### Target after Phase B

```toml
eframe = { version = "0.34", default-features = false, features = ["wgpu", "x11", "wayland"] }
egui = "0.34"
```

Rust edition: **2024**.

---

## 8. Working Practices for This Roadmap

### CI checks (from `AGENTS.md`/`CLAUDE.md`)

Before every commit:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo bench --no-run
```

Pre-commit hooks in `.claude/settings.json` enforce steps 1–4 automatically
when committing via Claude Code.

### Branch / commit conventions

- Branches: `feature/description`, `fix/description`, `perf/description`
- Commits: conventional (`feat:`, `fix:`, `perf:`, `refactor:`, `test:`,
  `docs:`, `chore:`) OR imperative short titles
- One logical change per commit
- Include `Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>` for
  AI-assisted commits
- Never push or open PRs without explicit user confirmation

### Per-phase PR checklist

- [ ] All CI green locally
- [ ] Manual smoke test: `cargo run --example color-gui-demo` (Phase A–B)
- [ ] Manual smoke test: the interactive binary on Windows native
- [ ] Manual smoke test: the interactive binary on WSL
- [ ] Manual smoke test: the interactive binary on native Linux (if
      available)
- [ ] Benchmark comparison if any hot path changed (Phase A, D)

---

## 9. Open Design Questions

These don't need answers now, but the Phase C agent should surface them:

1. **CLI shape.** Is interactive mode a new subcommand
   (`fractal-renderer interactive params.json`) or does it replace both
   `explore` and a future `color-editor`? My default recommendation:
   single `interactive` subcommand; `explore` becomes a deprecated alias.

2. **Params file round-trip.** Should color edits in the GUI be saveable
   back to the original JSON? What about fractal param edits? If yes, need
   a "Save As" workflow.

3. **Live-update architecture.** Per-frame texture upload vs. custom wgpu
   callback. Benchmark-driven decision (Phase A spike).

4. **Non-escape-time fractals** (Barnsley fern, pendulum): Phase D1's
   "color without re-render" only works for fractals with a separable
   compute/color pipeline. Others need full re-render on color change. Is
   that UX acceptable, or should we cache their final-pixel buffers
   differently?

5. **Headless mode coexistence.** Phase C introduces an `interactive` CLI
   entry; the headless `render` command is untouched. Confirm no shared
   code paths break (likely safe since `Renderable` is already shared).

---

## 10. Quick Start for a New Agent

If you're an agent picking up Phase A:

1. Read `src/core/user_interface.rs` end-to-end — understand the existing
   event loop and how `pixels` is used.
2. Read `src/core/color_map_editor_ui.rs` end-to-end — this is your
   reference for the `eframe::App` pattern in this codebase.
3. Read `src/core/render_window.rs` to understand `PixelGrid::draw()` which
   currently takes `&mut [u8]` from `pixels.frame_mut()`.
4. Start with the spike from Phase A's "De-risk first" section.
5. Implement the port in small commits; keep the old code compiling as
   long as possible to allow side-by-side testing.

If you're picking up Phase B:

1. Confirm Phase A is on `main` (no `pixels` or direct `winit` deps).
2. Follow §3.4's API shift table mechanically.
3. Edition 2024 errors: fix them one by one, don't reformat preemptively.

If you're picking up Phase C or D:

1. Confirm the previous phase is on `main`.
2. Re-read §5's scope for your phase.
3. Revisit §9's open questions and propose answers before writing code.

Good luck.
