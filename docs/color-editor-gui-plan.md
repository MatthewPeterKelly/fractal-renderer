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
egui-winit   = "0.22"     # use full default features for full event handling

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

### `color_map_editor_ui.rs` — Full rewrite

**Keep from existing code (if any):**
- `PREVIEW_W`, `EDITOR_W`, `TOTAL_W`, `TOTAL_H` layout constants
- `spawn_render` (background thread logic unchanged)
- `scale_preview` (fractal resolution scaling unchanged)
- `draw_preview` (blits fractal pixels into left pane, unchanged)

**Remove:**
- All tiny-skia imports and color constants
- All fontdue imports (e.g., `HACK_FONT_BYTES`)

**Add — persistent state structure:**

```rust
/// Persistent UI state for hello-world widgets (PR1)
/// Each field demos a widget type needed for PR2/PR3
struct EditorState {
    // Slider demo — will track position in PR2/PR3
    position_slider: f32,
    
    // Text input demo — will track position in PR2/PR3
    position_text: String,
    
    // Color picker demo — will track keyframe color in PR2/PR3
    color_picker_rgb: [u8; 3],
    
    // Drag-value demo — will track numeric edits in PR2/PR3
    drag_numeric: f32,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            position_slider: 0.5,
            position_text: "0.5".to_string(),
            color_picker_rgb: [128, 128, 128],
            drag_numeric: 1.0,
        }
    }
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

// Screen descriptor for egui rendering (based on window size)
let mut screen_descriptor = egui_wgpu::ScreenDescriptor {
    size_in_pixels: [TOTAL_W as u32, TOTAL_H as u32],
    pixels_per_point: window.scale_factor() as f32,
};
```

**Event loop changes:**

```rust
// On WindowEvent::Resized or whenever window size changes:
screen_descriptor = egui_wgpu::ScreenDescriptor {
    size_in_pixels: [new_width as u32, new_height as u32],
    pixels_per_point: window.scale_factor() as f32,
};

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

            // 1. Gradient bar (custom painter — same as before)
            ui.label("Current color map:");
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(EDITOR_W as f32 - 32.0, 44.0),
                egui::Sense::hover(),
            );
            paint_gradient_bar(ui.painter(), rect, keyframes);
            ui.separator();

            // 2. Slider demo (will track keyframe position in PR2/PR3)
            ui.label("Position slider (demo):");
            ui.add(egui::Slider::new(&mut state.position_slider, 0.0..=1.0)
                .step_by(0.01));
            ui.separator();

            // 3. Text input demo (will track numeric position in PR2/PR3)
            ui.label("Position text input (demo):");
            ui.text_edit_singleline(&mut state.position_text);
            ui.separator();

            // 4. Color picker demo (will track keyframe color in PR2/PR3)
            ui.label("Color picker (demo):");
            ui.color_edit_button_srgb(&mut state.color_picker_rgb);
            ui.separator();

            // 5. Drag-value demo (will track numeric edits in PR2/PR3)
            ui.label("Drag-value numeric (demo):");
            ui.add(egui::DragValue::new(&mut state.drag_numeric)
                .speed(0.01)
                .range(0.0..=1.0));
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

### Keyframe sourcing

Keyframes are read from the existing app state/config (loaded by the `color-map-editor` subcommand). PR1 displays them read-only in `paint_gradient_bar`. In PR2, `EditorState` will hold a mutable copy; in PR3, changes will flow back to the renderer via `set_keyframes`.

### `MainEventsCleared` scheduling (updated for egui)

Original behavior: Poll at 16 ms while a background render is in progress; sleep otherwise.

With egui: Also respect `output.repaint_after` from the egui context after each frame:
```rust
// After rendering, check if egui requests a repaint
match output.repaint_after {
    std::time::Duration::ZERO => {
        // Repaint immediately (e.g., hovering over widgets)
        window.request_redraw();
        control_flow.set_poll_at(Instant::now());
    }
    d => {
        // Repaint after delay if not already polling
        if !render_busy && !has_active_keys {
            control_flow.set_wait_until(Instant::now() + d);
        }
    }
}
```

### What is explicitly NOT in PR 1

- **No interaction between widgets and renderer** — `EditorState` is local; changes do not call `set_keyframes` or trigger renders
- **No per-keyframe editor rows** — saved for PR2 (layout with one row per keyframe)
- **No add/remove keyframe buttons** — saved for PR2
- **No live preview updates** — fractals render on a background schedule, not on widget input
- **No fractals-as-egui-texture** — fractal preview stays in the `pixels` framebuffer; egui panels composite on top with `LoadOp::Load`
- **No validation or constraints** — widgets accept any input; PR2/PR3 will add bounds checking

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

---

## Implementation Notes

### Window size constraints

The layout assumes `TOTAL_W = 1500` and `TOTAL_H = 480` are fixed. If the window is resizable, ensure `screen_descriptor` updates on `WindowEvent::Resized` and clamp the render resolution appropriately.

### Keyframe storage: read-only vs mutable

- **PR1**: Pass keyframes as `&[ColorMapKeyFrame]` (read-only reference from app state)
- **PR2**: Copy keyframes into `EditorState` as a mutable `Vec<ColorMapKeyFrame>`
- **PR3**: Sync mutations back to the renderer on each change

### Texture management

egui's texture atlas updates are handled automatically via `output.textures_delta.set` and `.free`. Do not manually manage egui textures beyond what the plan shows.

### Event consumption

Always check `egui_state.on_event` response and skip other event handling if egui consumed the event (e.g., text input within an egui widget should not also pan the fractal).

### Performance: first-frame stutter

egui's font rasterization may cause a hitch on the first frame. This is normal and will smooth out after the first few frames as glyphs are cached.
