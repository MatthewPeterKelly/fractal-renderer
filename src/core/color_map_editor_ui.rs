//! Color map editor UI using egui + eframe.
//!
//! This module provides the infrastructure for an interactive color map editor.
//! The public API is consumed by example binaries (e.g. `color-gui-demo`) and
//! will later be wired into a `color-map-editor` CLI subcommand. Until then,
//! nothing in the main binary calls into this module, so suppress dead-code warnings.
#![allow(dead_code)]

use image::Rgb;

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

/// Application state for the eframe-based color editor.
struct ColorEditorApp {
    /// Mutable widget state (sliders, text fields, color picker, etc.)
    editor_state: EditorState,
    /// Color map keyframes displayed in the gradient bar.
    keyframes: Vec<ColorMapKeyFrame>,
    /// GPU texture containing the fractal preview image.
    preview_texture: egui::TextureHandle,
}

impl ColorEditorApp {
    /// Create the app, converting the preview buffer to an egui texture and
    /// configuring the visual theme.
    fn new(
        cc: &eframe::CreationContext,
        preview_buffer: &[Vec<Rgb<u8>>],
        keyframes: Vec<ColorMapKeyFrame>,
    ) -> Self {
        // Black panel fill matches the clear color, eliminating sub-pixel gap
        // artifacts at panel boundaries (especially visible at non-integer DPI
        // or fullscreen). Disabling bg_stroke removes the 1px separator lines
        // egui draws between panels by default.
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::BLACK;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
        cc.egui_ctx.set_visuals(visuals);

        let image = buffer_to_color_image(preview_buffer);
        let texture =
            cc.egui_ctx
                .load_texture("fractal_preview", image, egui::TextureOptions::LINEAR);

        Self {
            editor_state: EditorState::default(),
            keyframes,
            preview_texture: texture,
        }
    }
}

impl eframe::App for ColorEditorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            || ctx.input(|i| i.key_pressed(egui::Key::Q))
        {
            frame.close();
        }

        build_editor_ui(
            ctx,
            &mut self.editor_state,
            &self.keyframes,
            &self.preview_texture,
        );
    }
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
        .default_width(EDITOR_W as f32)
        .width_range(200.0..=1200.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::BLACK)
                .inner_margin(egui::style::Margin::symmetric(8.0, 2.0)),
        )
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
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::BLACK)
                .inner_margin(egui::style::Margin::same(8.0)),
        )
        .show(ctx, |ui| {
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

/// Open the color map editor window with a pre-rendered fractal preview.
///
/// The preview is displayed as an egui-managed texture in a `CentralPanel`, while
/// the editor controls live in a `SidePanel`. eframe handles window management,
/// GPU rendering, input routing, DPI scaling, and resize — all cross-platform.
///
/// `preview_buffer` is a column-major grid of RGB pixels (as produced by
/// `Renderable::render_to_buffer`). It is converted to an egui texture at startup.
/// `keyframes` are displayed read-only in the gradient bar; the demo widgets are
/// independent and do not yet feed back into the renderer.
///
/// `preview_resolution` sets the initial window size; eframe handles dynamic
/// resizing from there.
pub fn run_color_editor(
    preview_buffer: Vec<Vec<Rgb<u8>>>,
    keyframes: Vec<ColorMapKeyFrame>,
    preview_resolution: [u32; 2],
) -> eframe::Result<()> {
    let initial_w = preview_resolution[0] as f32 + EDITOR_W as f32;
    let initial_h = preview_resolution[1] as f32;

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(initial_w, initial_h)),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Color Map Editor",
        options,
        Box::new(move |cc| Box::new(ColorEditorApp::new(cc, &preview_buffer, keyframes))),
    )
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
