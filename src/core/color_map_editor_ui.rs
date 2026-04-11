//! Color map editor UI using egui.
//!
//! This module provides the infrastructure for an interactive color map editor.
//! In PR1 (this module's initial version), functions are unused stubs that will be
//! integrated with the color-map-editor subcommand in future PRs.
#![allow(dead_code)]

use egui_wgpu::renderer::Renderer as EguiRenderer;
use egui_wgpu::renderer::ScreenDescriptor;
use egui_winit::State as EguiState;
use pixels::wgpu;
use pixels::Pixels;
use winit::event_loop::EventLoop;
use winit::window::Window;

use crate::core::color_map::{ColorMap, ColorMapKeyFrame, ColorMapper};
use crate::core::interpolation::LinearInterpolator;

/// Layout constants for the editor window
pub const PREVIEW_W: u32 = 640;
pub const EDITOR_W: u32 = 860;
pub const TOTAL_W: u32 = PREVIEW_W + EDITOR_W;
pub const TOTAL_H: u32 = 480;

/// Persistent UI state for the color map editor
/// Each field demonstrates a widget type needed for PR2/PR3
#[derive(Debug, Clone)]
pub struct EditorState {
    /// Slider demonstration — will track keyframe position in PR2/PR3
    pub position_slider: f32,

    /// Text input demonstration — will track keyframe position in PR2/PR3
    pub position_text: String,

    /// Color picker demonstration — will track keyframe color in PR2/PR3
    pub color_picker_rgb: [u8; 3],

    /// Drag-value demonstration — will track numeric edits in PR2/PR3
    pub drag_numeric: f32,
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

    let screen_descriptor = ScreenDescriptor {
        size_in_pixels: [TOTAL_W, TOTAL_H],
        pixels_per_point: 1.0,
    };

    (egui_ctx, egui_state, egui_renderer, screen_descriptor)
}

/// Update screen descriptor on window resize
pub fn update_screen_descriptor(screen_descriptor: &mut ScreenDescriptor, window: &Window) {
    screen_descriptor.size_in_pixels = [window.inner_size().width, window.inner_size().height];
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
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(EDITOR_W as f32 - 32.0, 44.0),
                egui::Sense::hover(),
            );
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

            // 4. Color picker demo (will track keyframe color in PR2/PR3)
            ui.label("Color picker (demo):");
            ui.color_edit_button_srgb(&mut state.color_picker_rgb);
            ui.separator();

            // 5. Drag-value demo (will track numeric edits in PR2/PR3)
            ui.label("Drag-value numeric (demo):");
            ui.add(
                egui::DragValue::new(&mut state.drag_numeric)
                    .speed(0.01)
                    .clamp_range(0.0..=1.0),
            );
        });
}

/// Paint a gradient bar showing the color map
fn paint_gradient_bar(painter: &egui::Painter, rect: egui::Rect, keyframes: &[ColorMapKeyFrame]) {
    if keyframes.is_empty() {
        return;
    }

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

/// Render a frame with egui composited on top of fractal preview
pub fn render_editor_frame(
    pixels: &mut Pixels,
    egui: &mut EguiRenderContext,
    window: &Window,
    editor_state: &mut EditorState,
    keyframes: &[ColorMapKeyFrame],
) -> Result<(), pixels::Error> {
    // Render fractal preview and egui
    pixels.render_with(|encoder, render_target, context| {
        // 1. Fractal pixels (placeholder for now — would call scaling_renderer here)
        // context.scaling_renderer.render(encoder, render_target);

        // 2. Build egui frame
        let raw_input = egui.state.take_egui_input(window);
        let output = egui.ctx.run(raw_input, |ctx| {
            build_editor_ui(ctx, editor_state, keyframes);
        });
        egui.state
            .handle_platform_output(window, egui.ctx, output.platform_output.clone());
        let paint_jobs = egui.ctx.tessellate(output.shapes.clone());

        // 3. Upload egui resources
        for (id, delta) in &output.textures_delta.set {
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
        for id in &output.textures_delta.free {
            egui.renderer.free_texture(id);
        }
        Ok(())
    })
}
