//! Interactive fractal explorer, built as an `eframe::App`.
//!
//! The app owns a background-rendered `PixelGrid` and blits its output into
//! an `egui` texture each time a new buffer is ready. Input is handled via
//! `egui::Context::input`; window management, DPI scaling, and resize are all
//! delegated to eframe.

use std::time::Duration;

use egui::{self, Color32, ColorImage, Frame, Key, Pos2, Rect, Sense};

use crate::core::{
    file_io::FilePrefix,
    image_utils::{ImageSpecification, PixelMapper, Renderable},
    render_window::{PixelGrid, RenderWindow},
    stopwatch::Stopwatch,
    view_control::{
        CenterCommand, CenterTargetCommand, CenterVelocityCommand, ScalarDirection, ViewControl,
        ZoomVelocityCommand,
    },
};

/// Base zoom rate, in units of natural-log-of-view-width per second.
const ZOOM_RATE: f64 = 0.4;
/// "Fast" zoom rate, triggered by the A/D keys when W/S are idle.
const FAST_ZOOM_RATE: f64 = 4.0 * ZOOM_RATE;
/// Pan rate while arrow keys are held, in view-widths per second.
const PAN_RATE: f64 = 0.2;
/// Pan rate when servoing toward a click target.
const FAST_PAN_RATE: f64 = 2.5 * PAN_RATE;

/// Minimum repaint period while the user is interacting or a render is in
/// flight. 100 Hz is faster than any common vsync cap, so the actual cadence
/// is still limited by the display refresh rate.
const ACTIVE_TICK: Duration = Duration::from_millis(10);

/// Defensive repaint period when the UI is otherwise idle. Keeps the app
/// responsive to silently-dropped resize / input events on WSL/XWayland
/// (see §4.2 of https://github.com/MatthewPeterKelly/fractal-renderer/blob/planning/gui-roadmap/docs/gui-unification-roadmap.md).
const IDLE_TICK: Duration = Duration::from_millis(100);

fn direction_from_key_pair(neg: bool, pos: bool) -> ScalarDirection {
    if neg == pos {
        ScalarDirection::Zero()
    } else if pos {
        ScalarDirection::Pos()
    } else {
        ScalarDirection::Neg()
    }
}

fn zoom_command_from_input(ctx: &egui::Context) -> ZoomVelocityCommand {
    ctx.input(|i| {
        let direction = direction_from_key_pair(i.key_down(Key::W), i.key_down(Key::S));
        if direction == ScalarDirection::Zero() {
            ZoomVelocityCommand {
                zoom_direction: direction_from_key_pair(i.key_down(Key::D), i.key_down(Key::A)),
                zoom_rate: FAST_ZOOM_RATE,
            }
        } else {
            ZoomVelocityCommand {
                zoom_direction: direction,
                zoom_rate: ZOOM_RATE,
            }
        }
    })
}

fn keyboard_center_command(ctx: &egui::Context) -> CenterCommand {
    ctx.input(|i| {
        let up_down = direction_from_key_pair(i.key_down(Key::ArrowDown), i.key_down(Key::ArrowUp));
        let left_right =
            direction_from_key_pair(i.key_down(Key::ArrowLeft), i.key_down(Key::ArrowRight));
        let velocity = CenterVelocityCommand {
            center_direction: [left_right, up_down],
            pan_rate: PAN_RATE,
        };
        if velocity.center_direction == [ScalarDirection::Zero(), ScalarDirection::Zero()] {
            CenterCommand::Idle()
        } else {
            CenterCommand::Velocity(velocity)
        }
    })
}

/// Convert a click in screen-space to a `CenterCommand` that recenters the
/// view on the fractal coordinate under the cursor.
fn click_to_center_command(
    click_pos: Pos2,
    image_rect: Rect,
    image_specification: &ImageSpecification,
) -> CenterCommand {
    let normalized_x = ((click_pos.x - image_rect.min.x) / image_rect.width()).clamp(0.0, 1.0);
    let normalized_y = ((click_pos.y - image_rect.min.y) / image_rect.height()).clamp(0.0, 1.0);

    let max_x = image_specification.resolution[0].saturating_sub(1);
    let max_y = image_specification.resolution[1].saturating_sub(1);
    let pixel = (
        ((normalized_x * image_specification.resolution[0] as f32) as u32).min(max_x),
        ((normalized_y * image_specification.resolution[1] as f32) as u32).min(max_y),
    );
    let (x, y) = PixelMapper::new(image_specification).map(&pixel);
    CenterCommand::Target(CenterTargetCommand {
        view_center: [x, y],
        pan_rate: FAST_PAN_RATE,
    })
}

fn any_control_key_held(ctx: &egui::Context) -> bool {
    const KEYS: &[Key] = &[
        Key::W,
        Key::A,
        Key::S,
        Key::D,
        Key::R,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
    ];
    ctx.input(|i| KEYS.iter().any(|k| i.key_down(*k)))
}

/// eframe application that drives the interactive fractal explorer.
struct ExploreApp<F: Renderable> {
    render_window: PixelGrid<F>,
    stopwatch: Stopwatch,
    texture: egui::TextureHandle,
    /// Reusable scratch buffer the render window copies into each time a new
    /// fractal image is ready. Allocated once to keep texture uploads off the
    /// allocator on the hot path.
    display_image: ColorImage,
}

impl<F: Renderable + Send + Sync + 'static> ExploreApp<F> {
    fn new(
        cc: &eframe::CreationContext<'_>,
        file_prefix: FilePrefix,
        image_specification: ImageSpecification,
        renderer: F,
    ) -> Self {
        // Match the color editor's theme: black panel fill + no separator
        // stroke avoids sub-pixel gap artifacts between panels at fractional
        // DPI (§4.1 of the roadmap).
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::BLACK;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
        cc.egui_ctx.set_visuals(visuals);

        let stopwatch = Stopwatch::new("Fractal Explorer".to_string());
        let time = stopwatch.total_elapsed_seconds();
        let render_window = PixelGrid::new(
            time,
            file_prefix,
            ViewControl::new(time, image_specification),
            renderer,
        );

        let [res_w, res_h] = image_specification.resolution;
        let display_image = ColorImage::new([res_w as usize, res_h as usize], Color32::BLACK);
        let texture = cc.egui_ctx.load_texture(
            "fractal_preview",
            display_image.clone(),
            egui::TextureOptions::LINEAR,
        );

        Self {
            render_window,
            stopwatch,
            texture,
            display_image,
        }
    }

    /// Show the fractal preview, centered in the available space with its
    /// aspect ratio preserved. Returns the click position (if any) along with
    /// the rect the image occupies on screen.
    fn show_preview(&self, ui: &mut egui::Ui) -> Option<(Pos2, Rect)> {
        let resolution = self.render_window.image_specification().resolution;
        let aspect = resolution[0] as f32 / resolution[1] as f32;
        let available = ui.available_size();
        let (display_w, display_h) = if available.x / available.y.max(1.0) > aspect {
            (available.y * aspect, available.y)
        } else {
            (available.x, available.x / aspect.max(f32::EPSILON))
        };

        let mut click = None;
        ui.centered_and_justified(|ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(display_w, display_h), Sense::click());
            ui.painter().image(
                self.texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    click = Some((pos, rect));
                }
            }
        });
        click
    }
}

impl<F: Renderable + 'static> eframe::App for ExploreApp<F> {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Q)) {
            frame.close();
            return;
        }

        if ctx.input(|i| i.key_down(Key::R)) {
            self.render_window.reset();
        }

        let click = egui::CentralPanel::default()
            .frame(Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| self.show_preview(ui))
            .inner;

        let image_specification = *self.render_window.image_specification();
        let center_command = match click {
            Some((pos, rect)) => click_to_center_command(pos, rect, &image_specification),
            None => keyboard_center_command(ctx),
        };
        let zoom_command = zoom_command_from_input(ctx);

        let time = self.stopwatch.total_elapsed_seconds();
        let new_buffer_ready = self
            .render_window
            .update(time, center_command, zoom_command);

        if new_buffer_ready {
            self.render_window.draw(&mut self.display_image);
            self.texture
                .set(self.display_image.clone(), egui::TextureOptions::LINEAR);
        }

        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.render_window.render_to_file();
        }

        // Keep the UI ticking while work is in flight or the user is driving
        // pan/zoom. When fully idle, fall back to a slow defensive repaint so
        // silently-dropped resize events on WSL/XWayland eventually recover.
        let active = any_control_key_held(ctx)
            || self.render_window.render_task_is_busy()
            || self.render_window.redraw_required()
            || self.render_window.adaptive_rendering_required();
        ctx.request_repaint_after(if active { ACTIVE_TICK } else { IDLE_TICK });
    }
}

/// Open the interactive fractal explorer window.
///
/// Controls:
/// - Arrow keys: pan the view.
/// - `W` / `S`: zoom in / out. Hold `A` / `D` (with no W/S) for a fast zoom.
/// - Left click: recenter the view on the clicked point.
/// - `R`: reset to the initial view.
/// - `Space`: save the current frame to disk (alongside its parameter JSON).
/// - `Esc` / `Q`: exit.
pub fn explore<F: Renderable + Send + Sync + 'static>(
    file_prefix: FilePrefix,
    image_specification: ImageSpecification,
    renderer: F,
) -> eframe::Result<()> {
    let [res_w, res_h] = image_specification.resolution;
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(res_w as f32, res_h as f32)),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Fractal Explorer",
        options,
        Box::new(move |cc| {
            Box::new(ExploreApp::new(
                cc,
                file_prefix,
                image_specification,
                renderer,
            ))
        }),
    )
}
