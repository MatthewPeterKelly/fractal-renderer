//! Interactive fractal explorer, built as an `eframe::App`.
//!
//! The app owns a background-rendered `PixelGrid` and blits its output into
//! an `egui` texture each time a new buffer is ready. Input is handled via
//! `egui::Context::input`; window management, DPI scaling, and resize are all
//! delegated to eframe.

use std::time::Duration;

use egui::{self, Color32, ColorImage, Frame, Key, Pos2, Rect, Sense};

use crate::core::{
    eframe_support::wgpu_native_options,
    file_io::FilePrefix,
    image_utils::{ImageSpecification, PixelMapper, Renderable},
    interactive::editor::{EditorState, delete_keyframe, show_palette_editor},
    render_window::{PixelGrid, RenderWindow, SnapshotSerializer},
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
const ACTIVE_TICK_DURATION: Duration = Duration::from_millis(10);

/// Defensive repaint period when the UI is otherwise idle. Keeps the app
/// responsive to silently-dropped resize / input events on WSL/XWayland
/// (see §4.2 of https://github.com/MatthewPeterKelly/fractal-renderer/blob/planning/gui-roadmap/docs/gui-unification-roadmap.md).
const IDLE_TICK_DURATION: Duration = Duration::from_millis(100);

/// Default width of the color-editor side panel, in logical pixels.
const EDITOR_PANEL_WIDTH: f32 = 260.0;
/// Resize bounds for the editor side panel. `size_range` (rather than a
/// fixed exact width) keeps the panel user-resizable (§4.3 of the roadmap).
const EDITOR_PANEL_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 180.0..=520.0;

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
struct FractalApp<F: Renderable> {
    render_window: PixelGrid<F>,
    stopwatch: Stopwatch,
    texture: egui::TextureHandle,
    /// Reusable scratch buffer the render window copies into each time a new
    /// fractal image is ready. Allocated once to keep texture uploads off the
    /// allocator on the hot path.
    display_image: ColorImage,
    /// Selection state for the color-editor side panel.
    editor_state: EditorState,
}

impl<F: Renderable + Send + Sync + 'static> FractalApp<F> {
    fn new(
        cc: &eframe::CreationContext<'_>,
        file_prefix: FilePrefix,
        image_specification: ImageSpecification,
        renderer: F,
        serialize_snapshot: SnapshotSerializer<F>,
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
            serialize_snapshot,
        );

        let [res_w, res_h] = image_specification.resolution;
        let display_image = ColorImage::filled([res_w as usize, res_h as usize], Color32::BLACK);
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
            editor_state: EditorState::default(),
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
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                click = Some((pos, rect));
            }
        });
        click
    }
}

impl<F: Renderable + 'static> eframe::App for FractalApp<F> {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // While a snapshot is saving, all interactive input is suppressed.
        // The view, palette, and selection are frozen
        // until the gated full-quality render completes.
        // Quit is *not* suppressed.
        let mut saving = self.render_window.is_saving();

        // Quit: `Q`, or `Ctrl+C` (terminal default).
        if ctx.input(|i| i.key_pressed(Key::Q) || (i.modifiers.ctrl && i.key_pressed(Key::C))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // `Space` initiates a gated, restorable save.
        // Debounced: a second press while a save is already in flight is ignored.
        if !saving && ctx.input(|i| i.key_pressed(Key::Space)) {
            self.render_window.request_save();
            // Suppress the rest of this frame's input immediately, so a
            // simultaneous slider drag or keypress on the press frame can't
            // mutate the state that is about to be snapshotted.
            saving = true;
        }

        // Keyframe-editor keys are inert while saving.
        if !saving {
            // `Esc` clears the keyframe selection (no-op when nothing selected).
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.editor_state.selected_keyframe = None;
            }

            // `Delete` removes the selected keyframe (no-op on the 0.0 / 1.0
            // anchors), then triggers a color-only re-render.
            if ctx.input(|i| i.key_pressed(Key::Delete))
                && let Some(selected) = self.editor_state.selected_keyframe
            {
                let active = self.editor_state.active_color_map;
                let mut deleted = false;
                {
                    let mut palette = self.render_window.palette().lock().unwrap();
                    if active < palette.color_maps.len() {
                        deleted = delete_keyframe(&mut palette.color_maps[active], selected);
                    }
                }
                if deleted {
                    self.editor_state.selected_keyframe = None;
                    self.render_window.mark_color_dirty();
                }
            }

            // `R` resets the view and the color palette to their initial state.
            // Edge-triggered (like Space): holding the key should reset once,
            // not re-clone the palette and re-mark the preview dirty every frame.
            if ctx.input(|i| i.key_pressed(Key::R)) {
                self.render_window.reset();
                self.editor_state.selected_keyframe = None;
            }
        }

        let mut palette_changed = false;
        egui::Panel::right("color_editor")
            .default_size(EDITOR_PANEL_WIDTH)
            .size_range(EDITOR_PANEL_WIDTH_RANGE)
            .show_separator_line(false)
            .frame(
                Frame::NONE
                    .fill(Color32::BLACK)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show_inside(ui, |ui| {
                // Disabled while saving so widget interactions cannot mutate
                // the palette mid-snapshot.
                ui.add_enabled_ui(!saving, |ui| {
                    let mut palette = self.render_window.palette().lock().unwrap();
                    palette_changed = show_palette_editor(&mut palette, ui, &mut self.editor_state);
                });
            });
        if palette_changed {
            self.render_window.mark_color_dirty();
            ctx.request_repaint();
        }

        let click = egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::BLACK))
            .show_inside(ui, |ui| self.show_preview(ui))
            .inner;

        // Suppress view commands while saving (Idle pan, zero zoom). The save
        // FSM still advances each `update`, and the frozen regulator resumes
        // afterward.
        let (center_command, zoom_command) = if saving {
            (CenterCommand::Idle(), ZoomVelocityCommand::zero())
        } else {
            let image_specification = *self.render_window.image_specification();
            let center_command = match click {
                Some((pos, rect)) => click_to_center_command(pos, rect, &image_specification),
                None => keyboard_center_command(&ctx),
            };
            (center_command, zoom_command_from_input(&ctx))
        };

        let time = self.stopwatch.total_elapsed_seconds();
        let new_buffer_ready = self
            .render_window
            .update(time, center_command, zoom_command);

        if new_buffer_ready {
            self.render_window.draw(&mut self.display_image);
            self.texture
                .set(self.display_image.clone(), egui::TextureOptions::LINEAR);
        }

        // "Saving snapshot…" overlay while the gated render is in flight.
        if self.render_window.is_saving() {
            draw_saving_overlay(&ctx);
        }

        // Keep the UI ticking while work is in flight or the user is driving
        // pan/zoom. When fully idle, fall back to a slow defensive repaint so
        // silently-dropped resize events on WSL/XWayland eventually recover.
        let active = any_control_key_held(&ctx)
            || self.render_window.render_task_is_busy()
            || self.render_window.redraw_required()
            || self.render_window.adaptive_rendering_required()
            || self.render_window.is_saving();
        ctx.request_repaint_after(if active {
            ACTIVE_TICK_DURATION
        } else {
            IDLE_TICK_DURATION
        });
    }
}

/// Paint a translucent, centered "Saving snapshot…" overlay while a gated save
/// render is in flight. Drawn as a foreground `Area`, so it sits above the
/// preview and editor without disturbing the §4.1 black-fill panel layout.
fn draw_saving_overlay(ctx: &egui::Context) {
    egui::Area::new(egui::Id::new("save_overlay"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            Frame::NONE
                .fill(Color32::from_black_alpha(220))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Saving snapshot…")
                            .size(20.0)
                            .color(Color32::WHITE),
                    );
                });
        });
}

/// Open the interactive fractal explorer window.
///
/// Controls:
/// - Arrow keys: pan the view.
/// - `W` / `S`: zoom in / out. Hold `A` / `D` (with no W/S) for a fast zoom.
/// - Left click: recenter the view on the clicked point.
/// - `R`: reset to the initial view and color palette.
/// - `Space`: save the current frame to disk (alongside its parameter JSON).
/// - Click a keyframe in the editor to edit its color; `+` inserts, the drag
///   values set segment widths.
/// - `Esc`: clear the keyframe selection. `Delete`: remove the selected
///   keyframe.
/// - `Q` / `Ctrl+C`: exit.
///
/// `serialize_snapshot` wraps the fractal's inner params back into a reloadable,
/// tagged `FractalParams` JSON string for the Space-as-save snapshot; the
/// dispatch site that selected the concrete `F` supplies it.
pub fn explore<F: Renderable + Send + Sync + 'static>(
    file_prefix: FilePrefix,
    image_specification: ImageSpecification,
    renderer: F,
    serialize_snapshot: impl Fn(&F::Params) -> String + 'static,
) -> eframe::Result<()> {
    let [res_w, res_h] = image_specification.resolution;
    let options = wgpu_native_options(
        egui::ViewportBuilder::default().with_inner_size([res_w as f32, res_h as f32]),
    );

    eframe::run_native(
        "Fractal Explorer",
        options,
        Box::new(move |cc| {
            Ok(Box::new(FractalApp::new(
                cc,
                file_prefix,
                image_specification,
                renderer,
                Box::new(serialize_snapshot),
            )))
        }),
    )
}
