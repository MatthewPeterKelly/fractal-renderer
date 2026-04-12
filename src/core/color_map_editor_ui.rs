//! Color map editor UI using egui.
//!
//! This module provides the infrastructure for an interactive color map editor.
//! The public API is consumed by example binaries (e.g. `color-gui-demo`) and
//! will later be wired into a `color-map-editor` CLI subcommand. Until then,
//! nothing in the main binary calls into this module, so suppress dead-code warnings.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use egui_wgpu::renderer::Renderer as EguiRenderer;
use egui_wgpu::renderer::ScreenDescriptor;
use egui_winit::State as EguiState;
use image::Rgb;
use pixels::wgpu;
use pixels::{Pixels, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, StartCause, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use crate::core::color_map::{ColorMap, ColorMapKeyFrame, ColorMapper};
use crate::core::interpolation::LinearInterpolator;

/// Minimum preview dimensions — guardrails to ensure the preview pane is usable.
const MIN_PREVIEW_W: u32 = 200;
const MIN_PREVIEW_H: u32 = 150;

/// Maximum preview dimensions — prevent absurdly large preview buffers.
const MAX_PREVIEW_W: u32 = 1920;
const MAX_PREVIEW_H: u32 = 1080;

/// Fixed width of the editor panel (right-hand side).
const EDITOR_W: u32 = 860;

/// Persistent UI state for the color map editor
/// Each field demonstrates a widget type needed for PR2/PR3
#[derive(Debug, Clone)]
pub struct EditorState {
    /// Slider demonstration — will track keyframe position in PR2/PR3
    pub position_slider: f32,

    /// Text input demonstration — will track keyframe position in PR2/PR3
    pub position_text: String,

    /// Color picker demonstration — will track keyframe color in PR2/PR3
    pub color_picker_color: egui::Color32,

    /// Drag-value demonstration — will track numeric edits in PR2/PR3
    pub drag_numeric: f32,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            position_slider: 0.5,
            position_text: "0.5".to_string(),
            color_picker_color: egui::Color32::from_rgb(128, 128, 128),
            drag_numeric: 1.0,
        }
    }
}

/// Bundled egui renderer state for cleaner function signatures
pub struct EguiRenderContext<'a> {
    pub ctx: &'a egui::Context,
    pub state: &'a mut EguiState,
    pub renderer: &'a mut EguiRenderer,
    pub screen_descriptor: &'a ScreenDescriptor,
}

/// Initialize egui context and renderer for the color map editor
pub fn init_egui(
    event_loop: &EventLoop<()>,
    pixels: &Pixels,
    window: &Window,
) -> (egui::Context, EguiState, EguiRenderer, ScreenDescriptor) {
    let egui_ctx = egui::Context::default();
    let mut egui_state = EguiState::new(event_loop);
    egui_state.set_max_texture_side(pixels.device().limits().max_texture_dimension_2d as usize);

    let egui_renderer = EguiRenderer::new(
        pixels.device(),
        pixels.render_texture_format(),
        None, // no depth buffer
        1,    // msaa_samples
    );

    let size = window.inner_size();
    let screen_descriptor = ScreenDescriptor {
        size_in_pixels: [size.width, size.height],
        pixels_per_point: window.scale_factor() as f32,
    };

    (egui_ctx, egui_state, egui_renderer, screen_descriptor)
}

/// Update screen descriptor on window resize
pub fn update_screen_descriptor(screen_descriptor: &mut ScreenDescriptor, window: &Window) {
    let size = window.inner_size();
    screen_descriptor.size_in_pixels = [size.width, size.height];
    screen_descriptor.pixels_per_point = window.scale_factor() as f32;
}

/// Build the editor UI with hello-world widgets
fn build_editor_ui(ctx: &egui::Context, state: &mut EditorState, keyframes: &[ColorMapKeyFrame]) {
    egui::SidePanel::right("editor")
        .exact_width(EDITOR_W as f32)
        .show(ctx, |ui| {
            ui.heading("Color Map Editor");
            ui.separator();

            // 1. Gradient bar (custom painter — same as before)
            ui.label("Current color map:");
            let bar_width = ui.available_width() - 16.0;
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(bar_width.max(100.0), 44.0), egui::Sense::hover());
            paint_gradient_bar(ui.painter(), rect, keyframes);
            ui.separator();

            // 2. Slider demo (will track keyframe position in PR2/PR3)
            ui.label("Position slider (demo):");
            ui.add(egui::Slider::new(&mut state.position_slider, 0.0..=1.0).step_by(0.01));
            ui.separator();

            // 3. Text input demo (will track numeric position in PR2/PR3)
            ui.label("Position text input (demo):");
            ui.text_edit_singleline(&mut state.position_text);
            ui.separator();

            // 4. Drag-value demo (will track numeric edits in PR2/PR3)
            ui.label("Drag-value numeric (demo):");
            ui.add(
                egui::DragValue::new(&mut state.drag_numeric)
                    .speed(0.01)
                    .clamp_range(0.0..=1.0),
            );

            // 5. Inline color picker anchored to the bottom of the panel.
            //    Using the embedded picker avoids popup focus issues with our
            //    custom event loop and keeps the color always visible for
            //    live feedback alongside the gradient bar.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.label("Color picker (demo):");
                egui::color_picker::color_picker_color32(
                    ui,
                    &mut state.color_picker_color,
                    egui::color_picker::Alpha::Opaque,
                );
            });
        });
}

/// Paint a gradient bar showing the color map
// TODO: cache the ColorMap between frames to avoid re-allocating the interpolator
// each repaint. For now the cost is negligible (small keyframe count), but it will
// matter once we support large keyframe sets or high refresh rates.
fn paint_gradient_bar(painter: &egui::Painter, rect: egui::Rect, keyframes: &[ColorMapKeyFrame]) {
    if keyframes.is_empty() {
        return;
    }

    let color_map = ColorMap::new(keyframes, LinearInterpolator {});
    let steps = (rect.width() as u32).max(2);
    // Render the gradient as adjacent 1-pixel-wide vertical line segments, each
    // filled with the interpolated color at that position. The query parameter t
    // is linearly spaced from 0.0 to 1.0 inclusive, so the first and last columns
    // show the exact boundary keyframe colors. We compute the reciprocal of
    // (steps - 1) once to avoid a division per iteration.
    let t_step = 1.0 / (steps - 1) as f32;
    for i in 0..steps {
        let t = i as f32 * t_step;
        let rgb = color_map.compute_pixel(t);
        let x = rect.left() + i as f32;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])),
        );
    }
}

/// Render a frame with egui composited on top of the fractal preview.
///
/// Returns the `Duration` that egui requests before the next repaint. A zero duration
/// means egui wants an immediate repaint (e.g. animation or hover); a longer duration
/// means the caller can sleep until then if nothing else requires a redraw.
pub fn render_editor_frame(
    pixels: &mut Pixels,
    egui: &mut EguiRenderContext,
    window: &Window,
    editor_state: &mut EditorState,
    keyframes: &[ColorMapKeyFrame],
) -> Result<Duration, pixels::Error> {
    let mut repaint_after = Duration::from_secs(1);

    pixels.render_with(|encoder, render_target, context| {
        // 1. Blit fractal pixels from the framebuffer to the render target
        context.scaling_renderer.render(encoder, render_target);

        // 2. Build egui frame
        let raw_input = egui.state.take_egui_input(window);
        let egui::FullOutput {
            platform_output,
            repaint_after: egui_repaint,
            textures_delta,
            shapes,
        } = egui.ctx.run(raw_input, |ctx| {
            build_editor_ui(ctx, editor_state, keyframes);
        });
        repaint_after = egui_repaint;
        egui.state
            .handle_platform_output(window, egui.ctx, platform_output);
        let paint_jobs = egui.ctx.tessellate(shapes);

        // 3. Upload egui resources
        for (id, delta) in &textures_delta.set {
            egui.renderer
                .update_texture(&context.device, &context.queue, *id, delta);
        }
        egui.renderer.update_buffers(
            &context.device,
            &context.queue,
            encoder,
            &paint_jobs,
            egui.screen_descriptor,
        );

        // 4. Composite egui over the frame (LoadOp::Load = no clear)
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: render_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
        });
        egui.renderer
            .render(&mut rpass, &paint_jobs, egui.screen_descriptor);
        drop(rpass);

        // 5. Free egui textures
        for id in &textures_delta.free {
            egui.renderer.free_texture(id);
        }
        Ok(())
    })?;

    Ok(repaint_after)
}

/// Copy a pre-rendered fractal preview into the left pane of the pixels framebuffer.
///
/// The preview is placed in the top-left corner and clipped to `preview_w x preview_h`.
/// The framebuffer stride is `total_w` (preview width + editor panel width).
/// If the buffer is smaller than the preview area, only the available pixels are copied
/// and the remainder stays black. If the buffer is larger, extra pixels are ignored.
/// This makes the function robust to mismatched fractal resolution in the params file
/// vs. the editor layout.
fn blit_preview_to_framebuffer(
    pixels: &mut Pixels,
    preview_buffer: &[Vec<Rgb<u8>>],
    preview_w: u32,
    preview_h: u32,
    total_w: u32,
) {
    let frame = pixels.frame_mut();
    let stride = total_w as usize;
    let cols = preview_buffer.len().min(preview_w as usize);
    for (x, col) in preview_buffer.iter().enumerate().take(cols) {
        let rows = col.len().min(preview_h as usize);
        for (y, rgb) in col.iter().enumerate().take(rows) {
            let idx = (y * stride + x) * 4;
            frame[idx] = rgb[0];
            frame[idx + 1] = rgb[1];
            frame[idx + 2] = rgb[2];
            frame[idx + 3] = 255;
        }
    }
}

/// Clamp a value to a `[min, max]` range.
fn clamp_dimension(val: u32, min: u32, max: u32) -> u32 {
    val.max(min).min(max)
}

/// Open the color map editor window with a pre-rendered fractal preview.
///
/// The preview pane dimensions are derived from `preview_resolution` (width, height),
/// which should come from the fractal's `image_specification.resolution`. The values
/// are clamped to `[MIN_PREVIEW_W, MAX_PREVIEW_W]` and `[MIN_PREVIEW_H, MAX_PREVIEW_H]`
/// to ensure a usable layout.
///
/// `preview_buffer` is a column-major grid of RGB pixels (as produced by
/// `Renderable::render_to_buffer`). It is blitted into the left pane once at startup.
/// `keyframes` are displayed read-only in the gradient bar; the demo widgets are
/// independent and do not yet feed back into the renderer.
pub fn run_color_editor(
    preview_buffer: Vec<Vec<Rgb<u8>>>,
    keyframes: Vec<ColorMapKeyFrame>,
    preview_resolution: [u32; 2],
) -> Result<(), pixels::Error> {
    let preview_w = clamp_dimension(preview_resolution[0], MIN_PREVIEW_W, MAX_PREVIEW_W);
    let preview_h = clamp_dimension(preview_resolution[1], MIN_PREVIEW_H, MAX_PREVIEW_H);
    let total_w = preview_w + EDITOR_W;
    let total_h = preview_h;

    // Use catch_unwind so that platforms without a display server (e.g. bare WSL)
    // produce a readable error instead of an opaque panic.
    let event_loop = std::panic::catch_unwind(EventLoop::new).unwrap_or_else(|_| {
        panic!("Failed to create EventLoop — is a display server available?");
    });
    let window = WindowBuilder::new()
        .with_title("Color Map Editor")
        .with_inner_size(LogicalSize::new(total_w as f64, total_h as f64))
        .build(&event_loop)
        .expect("failed to create window");

    let mut pixels = {
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, &window);
        Pixels::new(total_w, total_h, surface)?
    };

    blit_preview_to_framebuffer(&mut pixels, &preview_buffer, preview_w, preview_h, total_w);

    let (egui_ctx, mut egui_state, mut egui_renderer, mut screen_descriptor) =
        init_egui(&event_loop, &pixels, &window);
    let mut editor_state = EditorState::default();

    // Repaint scheduling is driven entirely by egui's repaint_after value
    // (handled in RedrawRequested). We intentionally omit a MainEventsCleared
    // handler — requesting a redraw there unconditionally would defeat
    // ControlFlow::WaitUntil and spin the CPU when the UI is idle.
    event_loop.run(move |event, _, control_flow| {
        if let Event::NewEvents(StartCause::Init) = event {
            *control_flow = ControlFlow::Wait;
        }

        if let Event::WindowEvent {
            event: ref window_event,
            ..
        } = event
        {
            let response = egui_state.on_event(&egui_ctx, window_event);
            if response.consumed {
                window.request_redraw();
                return;
            }

            match window_event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    if input.state == ElementState::Pressed {
                        if let Some(VirtualKeyCode::Escape | VirtualKeyCode::Q) =
                            input.virtual_keycode
                        {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                    }
                }
                WindowEvent::Resized(size) => {
                    if pixels.resize_surface(size.width, size.height).is_err() {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    // Resize the internal framebuffer so the scaling renderer
                    // matches the surface, preventing stale-resolution artifacts.
                    if size.width > 0 && size.height > 0 {
                        if pixels.resize_buffer(size.width, size.height).is_err() {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                        blit_preview_to_framebuffer(
                            &mut pixels,
                            &preview_buffer,
                            preview_w,
                            preview_h,
                            size.width,
                        );
                    }
                    update_screen_descriptor(&mut screen_descriptor, &window);
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    if pixels
                        .resize_surface(new_inner_size.width, new_inner_size.height)
                        .is_err()
                    {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    if new_inner_size.width > 0 && new_inner_size.height > 0 {
                        if pixels
                            .resize_buffer(new_inner_size.width, new_inner_size.height)
                            .is_err()
                        {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                        blit_preview_to_framebuffer(
                            &mut pixels,
                            &preview_buffer,
                            preview_w,
                            preview_h,
                            new_inner_size.width,
                        );
                    }
                    update_screen_descriptor(&mut screen_descriptor, &window);
                }
                _ => {}
            }
        }

        if let Event::RedrawRequested(_) = event {
            let mut egui_render = EguiRenderContext {
                ctx: &egui_ctx,
                state: &mut egui_state,
                renderer: &mut egui_renderer,
                screen_descriptor: &screen_descriptor,
            };
            match render_editor_frame(
                &mut pixels,
                &mut egui_render,
                &window,
                &mut editor_state,
                &keyframes,
            ) {
                Ok(repaint_after) => {
                    if repaint_after == Duration::ZERO {
                        window.request_redraw();
                    } else if let Some(deadline) = Instant::now().checked_add(repaint_after) {
                        *control_flow = ControlFlow::WaitUntil(deadline);
                    } else {
                        // repaint_after is Duration::MAX — egui has no pending
                        // animation; sleep until the next user event.
                        *control_flow = ControlFlow::Wait;
                    }
                }
                Err(_) => {
                    *control_flow = ControlFlow::Exit;
                }
            }
        }
    });
}
