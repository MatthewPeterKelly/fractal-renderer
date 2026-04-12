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

/// Convert a column-major `Vec<Vec<Rgb<u8>>>` preview buffer to a row-major
/// `egui::ColorImage` suitable for uploading as a GPU texture.
///
/// The source buffer uses `buffer[x][y]` indexing (column-major, as produced by
/// `Renderable::render_to_buffer`). The output `ColorImage` is row-major RGBA,
/// which is what egui expects.
fn buffer_to_color_image(buffer: &[Vec<Rgb<u8>>]) -> egui::ColorImage {
    let width = buffer.len();
    let height = buffer.first().map_or(0, |col| col.len());
    // Transpose from column-major (buffer[x][y]) to row-major (egui expects
    // pixels in left-to-right, top-to-bottom order).
    let pixels: Vec<egui::Color32> = (0..height)
        .flat_map(|y| {
            buffer
                .iter()
                .map(move |col| egui::Color32::from_rgb(col[y][0], col[y][1], col[y][2]))
        })
        .collect();
    egui::ColorImage {
        size: [width, height],
        pixels,
    }
}

/// Build the editor UI with hello-world widgets
fn build_editor_ui(
    ctx: &egui::Context,
    state: &mut EditorState,
    keyframes: &[ColorMapKeyFrame],
    preview_texture: &egui::TextureHandle,
) {
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

    // Preview panel: fills the remaining space to the left of the editor.
    // The fractal image is scaled to fit while preserving its aspect ratio.
    egui::CentralPanel::default().show(ctx, |ui| {
        let available = ui.available_size();
        let aspect = preview_texture.aspect_ratio();
        let (display_w, display_h) = if available.x / available.y.max(1.0) > aspect {
            (available.y * aspect, available.y)
        } else {
            (available.x, available.x / aspect.max(0.001))
        };
        ui.centered_and_justified(|ui| {
            ui.add(egui::Image::new(
                preview_texture,
                egui::vec2(display_w, display_h),
            ));
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
    preview_texture: &egui::TextureHandle,
) -> Result<Duration, pixels::Error> {
    let mut repaint_after = Duration::from_secs(1);

    pixels.render_with(|encoder, render_target, context| {
        // Build egui frame
        let raw_input = egui.state.take_egui_input(window);
        let egui::FullOutput {
            platform_output,
            repaint_after: egui_repaint,
            textures_delta,
            shapes,
        } = egui.ctx.run(raw_input, |ctx| {
            build_editor_ui(ctx, editor_state, keyframes, preview_texture);
        });
        repaint_after = egui_repaint;
        egui.state
            .handle_platform_output(window, egui.ctx, platform_output);
        let paint_jobs = egui.ctx.tessellate(shapes);

        // Upload egui resources
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

        // Render egui (LoadOp::Clear — egui owns the entire render target)
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: render_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
        });
        egui.renderer
            .render(&mut rpass, &paint_jobs, egui.screen_descriptor);
        drop(rpass);

        // Free egui textures
        for id in &textures_delta.free {
            egui.renderer.free_texture(id);
        }
        Ok(())
    })?;

    Ok(repaint_after)
}

/// Open the color map editor window with a pre-rendered fractal preview.
///
/// The preview is displayed as an egui-managed texture in a `CentralPanel`, while
/// the editor controls live in a `SidePanel`. egui owns the entire window layout,
/// so there are no coordinate-space mismatches on resize.
///
/// `preview_buffer` is a column-major grid of RGB pixels (as produced by
/// `Renderable::render_to_buffer`). It is converted to an egui texture at startup.
/// `keyframes` are displayed read-only in the gradient bar; the demo widgets are
/// independent and do not yet feed back into the renderer.
///
/// `preview_resolution` sets the initial window size; egui handles dynamic resizing
/// from there.
pub fn run_color_editor(
    preview_buffer: Vec<Vec<Rgb<u8>>>,
    keyframes: Vec<ColorMapKeyFrame>,
    preview_resolution: [u32; 2],
) -> Result<(), pixels::Error> {
    let initial_w = preview_resolution[0] + EDITOR_W;
    let initial_h = preview_resolution[1];

    // Use catch_unwind so that platforms without a display server (e.g. bare WSL)
    // produce a readable error instead of an opaque panic.
    let event_loop = std::panic::catch_unwind(EventLoop::new).unwrap_or_else(|_| {
        panic!("Failed to create EventLoop — is a display server available?");
    });
    let window = WindowBuilder::new()
        .with_title("Color Map Editor")
        .with_inner_size(LogicalSize::new(initial_w as f64, initial_h as f64))
        .build(&event_loop)
        .expect("failed to create window");

    // The pixels crate is used only for wgpu surface management. The 1x1
    // framebuffer is never drawn — egui owns the entire render target.
    let mut pixels = {
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, &window);
        Pixels::new(1, 1, surface)?
    };

    let (egui_ctx, mut egui_state, mut egui_renderer, mut screen_descriptor) =
        init_egui(&event_loop, &pixels, &window);

    let preview_image = buffer_to_color_image(&preview_buffer);
    let preview_texture = egui_ctx.load_texture(
        "fractal_preview",
        preview_image,
        egui::TextureOptions::LINEAR,
    );

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
                    update_screen_descriptor(&mut screen_descriptor, &window);
                    // Force an immediate redraw so egui repaints at the new
                    // surface size, preventing stale panel-border artifacts.
                    window.request_redraw();
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    if pixels
                        .resize_surface(new_inner_size.width, new_inner_size.height)
                        .is_err()
                    {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    update_screen_descriptor(&mut screen_descriptor, &window);
                    window.request_redraw();
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
                &preview_texture,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_to_color_image_transposition() {
        // 3 columns x 2 rows (column-major: buffer[x][y])
        let buffer = vec![
            vec![Rgb([10, 20, 30]), Rgb([40, 50, 60])],
            vec![Rgb([70, 80, 90]), Rgb([100, 110, 120])],
            vec![Rgb([130, 140, 150]), Rgb([160, 170, 180])],
        ];

        let image = buffer_to_color_image(&buffer);

        assert_eq!(image.size, [3, 2]);
        assert_eq!(image.pixels.len(), 6);

        // Row 0: buffer[0][0], buffer[1][0], buffer[2][0]
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(10, 20, 30));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(70, 80, 90));
        assert_eq!(image.pixels[2], egui::Color32::from_rgb(130, 140, 150));

        // Row 1: buffer[0][1], buffer[1][1], buffer[2][1]
        assert_eq!(image.pixels[3], egui::Color32::from_rgb(40, 50, 60));
        assert_eq!(image.pixels[4], egui::Color32::from_rgb(100, 110, 120));
        assert_eq!(image.pixels[5], egui::Color32::from_rgb(160, 170, 180));
    }

    #[test]
    fn test_buffer_to_color_image_empty() {
        let buffer: Vec<Vec<Rgb<u8>>> = vec![];
        let image = buffer_to_color_image(&buffer);
        assert_eq!(image.size, [0, 0]);
        assert!(image.pixels.is_empty());
    }
}
