# Color Map Editor GUI — Implementation Plan

## Context

This document captures the design decisions from the prototyping branch
(`color-gui/hello-world-gui`) and lays out the implementation plan for the
three-PR sequence that will build the interactive color map editor.

---

## Design Decisions (settled)

### Single window, single wgpu surface

`pixels 0.13` creates a full wgpu device per instance. lavapipe (WSL software
Vulkan) supports only one surface at a time, so two `Pixels` instances fail at
runtime. All rendering lives in one window.

### egui for all UI

The manual tiny-skia + fontdue drawing layer established in the prototype is
replaced by `egui`. Reasons:

- Immediate mode — widgets are declared inline each frame; no retained state
  graph or event routing to manage
- `egui-wgpu 0.22` targets `wgpu 0.16`, which is exactly what `pixels 0.13`
  uses; integration is a first-class supported pattern with an official example
  in the pixels repo
- Provides every widget needed: `Slider`, `DragValue`, `TextEdit`,
  `color_edit_button_rgb`, `ui.label`, and a custom `Painter` for the gradient
  bar
- Eliminates the need for an embedded font, `tiny-skia`, `fontdue`, and all
  manual glyph/pixel blitting code

### Compositing model

```
pixels.render_with(|encoder, render_target, context| {
    // 1. Blit fractal pixels (left pane) via scaling_renderer
    context.scaling_renderer.render(encoder, render_target);
    // 2. egui composited on top with LoadOp::Load (does not clear)
    egui_renderer.render(&mut rpass, &paint_jobs, &screen_descriptor);
})
```

The fractal preview is written directly into the `pixels` framebuffer. The
egui panel renders on top with an opaque background, covering the right half.
No texture upload or `egui::Image` is required yet.

### Fractal renders at preview resolution

`scale_preview` downsizes the `ImageSpecification` before handing it to the
renderer, so `render_to_buffer` works at `PREVIEW_W × TOTAL_H` rather than the
original full resolution. The preview is centred in its pane with letterboxing;
the surrounding area is cleared to black each frame.

### Background render thread (already established)

```
bg thread:    renderer.render_to_buffer() → sets render_ready flag
main thread:  MainEventsCleared: if render_ready → request_redraw
```

---

## Exact Dependency Versions

```toml
# Add
egui         = "0.22"
egui-wgpu    = "0.22"
egui-winit   = { version = "0.22", default-features = false, features = ["links"] }

# Remove
tiny-skia    = "0.12"   # replaced by egui Painter
fontdue      = "0.9"    # replaced by egui text
```

Remove from the repo:
- `assets/Hack-Regular.ttf`
- `THIRD_PARTY_LICENSES.md` (Hack attribution no longer needed)

---

## Three-PR Sequence

### PR 1 — egui hello world (this document)
Replace the drawing layer with egui. Prove the integration works. Show one of
each widget type. No real interaction.

### PR 2 — Full editor layout
Real color editor layout: gradient bar, per-keyframe rows (color swatch,
position slider, position text input), add/remove buttons. Still no
interaction with the renderer.

### PR 3 — Live interaction
Wire keyframe edits to `set_keyframes` on the renderer. Trigger a new
background render on every change. The fractal preview updates live as the
user drags a slider.

---

## PR 1 Detailed Plan

### Goal

Replace the drawing layer. The UI shows four hello-world widgets in the right
pane with no real logic attached, alongside the live fractal preview in the
left pane.

### Window layout (unchanged constants)

```
┌──────────────────────┬────────────────────────┐
│   fractal preview    │     egui editor pane   │
│   PREVIEW_W × H      │     EDITOR_W × H       │
│      640 px          │        860 px          │
└──────────────────────┴────────────────────────┘
                         TOTAL_W = 1500, TOTAL_H = 480
```

### Cargo.toml changes

- Add `egui = "0.22"`, `egui-wgpu = "0.22"`, `egui-winit = "0.22"`
- Remove `tiny-skia = "0.12"`, `fontdue = "0.9"`

### Files changed

| File | Change |
|---|---|
| `Cargo.toml` | Swap deps as above |
| `assets/Hack-Regular.ttf` | Delete |
| `THIRD_PARTY_LICENSES.md` | Delete |
| `src/core/color_map_editor_ui.rs` | Full rewrite (see below) |

No other files change — `color_map.rs`, `cli/color_map_editor.rs`, the example,
and the rest of the renderer are untouched.

### `color_map_editor_ui.rs` rewrite

**Keep:**
- `PREVIEW_W`, `EDITOR_W`, `TOTAL_W`, `TOTAL_H` layout constants
- `spawn_render` (background thread logic unchanged)
- `scale_preview` (fractal resolution scaling unchanged)
- `draw_preview` (blits fractal pixels into left pane, unchanged)

**Remove:**
- `draw_text` — egui handles all text
- `draw_editor` — replaced by `build_editor_ui`
- All tiny-skia imports and color constants
- All fontdue imports and `HACK_FONT_BYTES`

**Add:**

```rust
// Persistent state for hello-world widgets
struct EditorState {
    slider_value: f32,
    text_value: String,
}
```

```rust
// egui setup (constructed once in `edit()`, before the event loop)
let egui_ctx = egui::Context::default();
let mut egui_state = egui_winit::State::new(&event_loop);
egui_state.set_max_texture_side(
    pixels.device().limits().max_texture_dimension_2d as usize
);
let mut egui_renderer = egui_wgpu::renderer::Renderer::new(
    pixels.device(),
    pixels.render_texture_format(),
    None,  // no depth buffer
    1,     // msaa_samples
);
```

**Event loop changes:**

```rust
// In WindowEvent branch — pass events to egui first
let response = egui_state.on_event(&egui_ctx, &window_event);
if response.consumed { return; }
// then existing Q/Escape/CloseRequested handling
```

**Per-frame render with `render_with`:**

```rust
pixels.render_with(|encoder, render_target, context| {
    // 1. Fractal pixels
    context.scaling_renderer.render(encoder, render_target);

    // 2. Build egui frame
    let raw_input = egui_state.take_egui_input(&window);
    let output = egui_ctx.run(raw_input, |ctx| {
        build_editor_ui(ctx, &mut editor_state, keyframes);
    });
    egui_state.handle_platform_output(&window, &egui_ctx, output.platform_output);
    let paint_jobs = egui_ctx.tessellate(output.shapes);

    // 3. Upload egui resources
    for (id, delta) in &output.textures_delta.set {
        egui_renderer.update_texture(context.device, context.queue, *id, delta);
    }
    egui_renderer.update_buffers(
        context.device, context.queue, encoder, &paint_jobs, &screen_descriptor
    );

    // 4. Composite egui over the frame (LoadOp::Load = no clear)
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: render_target,
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: true },
            ..
        })],
        ..
    });
    egui_renderer.render(&mut rpass, &paint_jobs, &screen_descriptor);
    drop(rpass);

    for id in &output.textures_delta.free {
        egui_renderer.free_texture(id);
    }
    Ok(())
})?;
```

**`build_editor_ui` — hello-world widgets:**

```rust
fn build_editor_ui(ctx: &egui::Context, state: &mut EditorState, keyframes: &[ColorMapKeyFrame]) {
    egui::SidePanel::right("editor")
        .exact_width(EDITOR_W as f32)
        .show(ctx, |ui| {

            ui.heading("Color Map Editor");
            ui.separator();

            // 1. Static label
            ui.label("Hello from egui — this text is a widget.");
            ui.separator();

            // 2. Gradient bar (custom painter, same column-loop as before)
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(EDITOR_W as f32 - 32.0, 44.0),
                egui::Sense::hover(),
            );
            paint_gradient_bar(ui.painter(), rect, keyframes);
            ui.separator();

            // 3. Slider
            ui.label("Dummy slider:");
            ui.add(egui::Slider::new(&mut state.slider_value, 0.0..=1.0));
            ui.separator();

            // 4. Numeric text entry
            ui.label("Numeric text entry:");
            ui.add(egui::TextEdit::singleline(&mut state.text_value)
                .hint_text("enter a number"));
        });
}
```

**`paint_gradient_bar` — custom painter:**

```rust
fn paint_gradient_bar(painter: &egui::Painter, rect: egui::Rect, keyframes: &[ColorMapKeyFrame]) {
    if keyframes.is_empty() { return; }
    let color_map = ColorMap::new(keyframes, LinearInterpolator {});
    let steps = rect.width() as u32;
    for i in 0..steps {
        let t = i as f32 / steps.max(1) as f32;
        let rgb = color_map.compute_pixel(t);
        let x = rect.left() + i as f32;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])),
        );
    }
}
```

### `MainEventsCleared` scheduling (unchanged)

Poll at 16 ms while a background render is in progress; sleep otherwise. If
egui requests a repaint (`output.repaint_after == Duration::ZERO`), call
`window.request_redraw()`.

### What is explicitly NOT in PR 1

- No interaction between the widgets and the renderer
- No color picker
- No per-keyframe rows
- No add/remove keyframe buttons
- No live preview update on slider drag
- No fractal-as-egui-texture (fractal stays in the pixels buffer directly)

---

## PR 2 Preview (for context)

PR 2 builds the real editor layout on top of the egui skeleton:

- Gradient bar at the top (same painter, real keyframes)
- One row per keyframe: color swatch | position slider | position text input
- `+` / `−` buttons to add and remove keyframes
- HSV/RGB color picker (`egui::color_edit_button_rgb`) per keyframe

No interaction with the renderer yet — state changes are local to `EditorState`.

---

## PR 3 Preview (for context)

PR 3 wires the editor state to the renderer:

- `ColorMapEditable::set_keyframes` (to be re-added) called whenever
  `EditorState` changes
- Any change triggers `spawn_render` (existing background thread mechanism)
- `render_busy` flag prevents double-spawning during a slow render
- Speed optimizer / downsampled preview during drag, full quality on release
