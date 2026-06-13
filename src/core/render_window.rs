use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use egui::{Color32, ColorImage};

use crate::core::color_map::ColorPalette;
use crate::core::render_quality_fsm::AdaptiveOptimizationRegulator;

use super::{
    file_io::{FilePrefix, date_time_string, write_file_or_panic},
    image_utils::{
        ImageSpecification, Renderable, color_image_to_rgb8, field_upsample_factor,
        write_image_to_file_or_panic,
    },
    render_pipeline::RenderingPipeline,
    view_control::{CenterCommand, CenterTargetCommand, ViewControl, ZoomVelocityCommand},
};

/// A trait for managing and rendering a graphical view with controls for recentering,
/// panning, zooming, updating, and saving the rendered output. This is the core interface
/// used by the "explore" GUI to interact with the different fractals.
pub trait RenderWindow {
    /// Provides access to the current image specification for the window
    fn image_specification(&self) -> &ImageSpecification;

    /// Resets the render window back to the view port that it was initialized with.
    fn reset(&mut self);

    /// Recompute the entire fractal if any internal parameters have changed. This should be
    /// a no-op if called with no internal changes.
    ///
    /// # Return: true if the buffer was updated, false if the call was a no-op.
    fn update(
        &mut self,
        time: f64,
        center_command: CenterCommand,
        zoom_command: ZoomVelocityCommand,
    ) -> bool;

    /// Copies the latest rendered buffer into an `egui::ColorImage` suitable for
    /// uploading to a `TextureHandle`. Typically `update()` is called before
    /// `draw()`; the call clears the internal "redraw required" flag.
    ///
    /// # Parameters
    ///
    /// - `image`: A mutable `ColorImage` whose `size` matches the render
    ///   resolution. Its row-major pixel buffer is overwritten in place with
    ///   the latest fractal colors.
    fn draw(&self, image: &mut ColorImage);
}

/// State of the gated Space-as-save flow (Phase 5 of the GUI roadmap): a
/// deliberate "publish this exact state" action that forces a full-quality
/// render before writing a reloadable params JSON + a matching PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveState {
    /// No save in progress.
    Idle,
    /// Space pressed; waiting for the render worker to be free so a forced
    /// full-quality render can be launched.
    Pending,
    /// A full-quality save render is in flight; the snapshot is written to
    /// disk once it completes.
    Rendering,
}

/// Boxed closure that wraps a fractal's inner params into a reloadable, tagged
/// `FractalParams` JSON string for the Space-as-save snapshot. It is `dyn`
/// (off the render hot path) so `core` need not name the
/// `fractals::FractalParams` enum; the dispatch site that picked the concrete
/// variant supplies it.
pub type SnapshotSerializer<F> = Box<dyn Fn(&<F as Renderable>::Params) -> String>;

/// Generic `RenderWindow` implementation backed by a `RenderingPipeline`.
/// All "solve by pixel" fractals run through it. Per-(sub)pixel dispatch is
/// fully monomorphized over `F: Renderable`; the `dyn` boundary stays on
/// the GUI side.
pub struct PixelGrid<F: Renderable> {
    // Output `egui::ColorImage` written by the pipeline on the background
    // thread, then copied into the eframe-supplied texture in `draw`.
    display_buffer: Arc<Mutex<ColorImage>>,

    // Interprets UI commands to pan and zoom, translating them into the
    // image specification used by the renderer.
    view_control: ViewControl,

    // File prefix for snapshot writes.
    file_prefix: FilePrefix,

    // Measures render time and adaptively trades render quality for speed
    // while the user is interacting; ramps quality back up while idle.
    adaptive_quality_regulator: AdaptiveOptimizationRegulator,

    // Pipeline owning the fractal, field, histogram, CDF, and color cache.
    // Mutex-wrapped so the GUI thread can adjust speed-optimization level
    // and the background thread can render in parallel.
    pipeline: Arc<Mutex<RenderingPipeline<F>>>,

    // Cache of the user's full-quality reference parameters; used by the
    // regulator to interpolate runtime values back toward the user's
    // specified state.
    speed_optimizer_cache: F::ReferenceCache,

    // Lock ensuring only one background render runs at a time.
    render_task_is_busy: Arc<AtomicBool>,

    // Flag set by the background thread when a fresh image is ready in
    // `display_buffer` and needs to be uploaded to the texture.
    redraw_required: Arc<AtomicBool>,

    // Set by the editor (UI thread) when a keyframe / background edit needs
    // a color-only re-render. Drained by `update`, which spawns a
    // `recolorize` task (a full view render takes priority).
    color_dirty: Arc<AtomicBool>,

    // Editor's source-of-truth color palette, held in its own lightweight
    // mutex so the UI thread can read/mutate it every frame without ever
    // locking the (long-held) pipeline mutex. Synced into the fractal at the
    // start of each render / recolorize. See the Phase-4 roadmap deviation
    // note: this avoids freezing the editor during a long render.
    palette: Arc<Mutex<ColorPalette>>,

    // The palette the fractal was constructed with; restored by `reset` so
    // `R` returns to the initial colors as well as the initial view.
    initial_color_palette: ColorPalette,

    // Runtime `sampling_level` of the most recently completed full render.
    // `recolorize` reuses it so the color-only pass walks the same populated
    // sub-rectangle of the field that the last compute pass filled.
    last_sampling_level: Arc<AtomicI32>,

    // Whether a render has ever been launched on this `PixelGrid`.
    has_started_rendering: bool,

    // Serializes the current fractal params into a reloadable, tagged
    // `FractalParams` JSON string for the Space-as-save snapshot. Boxed so
    // `core` need not depend on the `fractals::FractalParams` enum; supplied
    // by the dispatch site that knows the concrete variant.
    serialize_snapshot: SnapshotSerializer<F>,

    // Drives the gated Space-as-save flow (see `SaveState`).
    save_state: SaveState,
}

const TARGET_RENDER_FRAMES_PER_SECOND: f64 = 24.0;

impl<F> PixelGrid<F>
where
    F: Renderable + Send + Sync + 'static,
{
    /// Construct a `PixelGrid` around the given fractal. Allocates the
    /// pipeline's reusable buffers at the user's full sampling level.
    ///
    /// `serialize_snapshot` wraps the fractal's inner params back into a
    /// reloadable, tagged `FractalParams` JSON string for the Space-as-save
    /// snapshot (kept out of `core` so the layering stays `fractals → core`).
    pub fn new(
        time: f64,
        file_prefix: FilePrefix,
        view_control: ViewControl,
        renderer: F,
        serialize_snapshot: SnapshotSerializer<F>,
    ) -> Self {
        let resolution = view_control.image_specification().resolution;
        let center_command = CenterCommand::Target(CenterTargetCommand {
            view_center: view_control.image_specification().center,
            pan_rate: 0.0,
        });

        let n_max_plus_1 = field_upsample_factor(renderer.render_options().sampling_level);
        let bin_count = renderer.histogram_bin_count();
        let hist_max = renderer.histogram_max_value();
        let lut_count = renderer.lookup_table_count();
        let speed_optimizer_cache = renderer.reference_cache();
        let initial_color_palette = renderer.color_palette().clone();
        let pipeline =
            RenderingPipeline::new(renderer, n_max_plus_1, bin_count, hist_max, lut_count);
        let display_buffer = ColorImage::filled(
            [resolution[0] as usize, resolution[1] as usize],
            Color32::BLACK,
        );

        let mut pixel_grid = Self {
            display_buffer: Arc::new(Mutex::new(display_buffer)),
            view_control,
            file_prefix,
            pipeline: Arc::new(Mutex::new(pipeline)),
            speed_optimizer_cache,
            render_task_is_busy: Arc::new(AtomicBool::new(false)),
            redraw_required: Arc::new(AtomicBool::new(false)),
            color_dirty: Arc::new(AtomicBool::new(false)),
            palette: Arc::new(Mutex::new(initial_color_palette.clone())),
            initial_color_palette,
            last_sampling_level: Arc::new(AtomicI32::new(0)),
            has_started_rendering: false,
            serialize_snapshot,
            save_state: SaveState::Idle,
            adaptive_quality_regulator: AdaptiveOptimizationRegulator::new(
                1.0 / TARGET_RENDER_FRAMES_PER_SECOND,
            ),
        };
        pixel_grid
            .view_control
            .update(time, center_command, ZoomVelocityCommand::zero());
        pixel_grid
    }

    /// Whether a background render is currently in flight.
    pub fn render_task_is_busy(&self) -> bool {
        self.render_task_is_busy.load(Ordering::Acquire)
    }

    /// Whether a completed render is waiting to be drawn.
    pub fn redraw_required(&self) -> bool {
        self.redraw_required.load(Ordering::Acquire)
    }

    /// Whether the regulator wants another render even without user input
    /// (e.g. ramping quality back up after the user stopped panning).
    pub fn adaptive_rendering_required(&self) -> bool {
        !self.adaptive_quality_regulator.is_idle()
    }

    /// Editor's source-of-truth color palette. The interactive app locks
    /// this each frame to draw and mutate the palette; edits are picked up by
    /// the next render / recolorize, which copies it into the fractal.
    pub fn palette(&self) -> &Arc<Mutex<ColorPalette>> {
        &self.palette
    }

    /// Signal that the palette was edited and the preview needs a color-only
    /// re-render. Drained by the next `update`.
    pub fn mark_color_dirty(&self) {
        self.color_dirty.store(true, Ordering::Release);
    }

    /// Whether a gated Space-as-save snapshot is currently being produced.
    /// While true, the interactive app locks input and shows a "Saving…"
    /// overlay.
    pub fn is_saving(&self) -> bool {
        self.save_state != SaveState::Idle
    }

    /// Begin a gated Space-as-save: the next `update` calls force a
    /// full-quality render and then write a reloadable params JSON + a
    /// matching PNG. No-op if a save is already in progress (this debounces a
    /// double Space press).
    pub fn request_save(&mut self) {
        if self.save_state == SaveState::Idle {
            self.save_state = SaveState::Pending;
        }
    }

    /// Serialize the current (full-quality, palette- and view-synced) fractal
    /// params to a timestamped reloadable JSON and write the on-screen buffer
    /// to a matching PNG. Called once the forced save render completes.
    fn write_snapshot(&self) {
        let datetime = date_time_string();
        let json = {
            let pipeline = self.pipeline.lock().unwrap();
            (self.serialize_snapshot)(pipeline.fractal().params())
        };
        write_file_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{datetime}.json")),
            &json,
        );
        let imgbuf = color_image_to_rgb8(&self.display_buffer.lock().unwrap());
        write_image_to_file_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{datetime}.png")),
            |f| imgbuf.save(f),
        );
    }

    /// Spawn a background render on the pipeline. The pipeline's mutex
    /// serializes against the UI thread's parameter edits.
    fn render(&mut self) {
        let display_buffer = self.display_buffer.clone();
        let pipeline = self.pipeline.clone();
        let palette = self.palette.clone();
        let image_specification = *self.image_specification();
        let render_task_is_busy = Arc::clone(&self.render_task_is_busy);
        let redraw_required = self.redraw_required.clone();
        let last_sampling_level = self.last_sampling_level.clone();

        std::thread::spawn(move || {
            let mut color_image = display_buffer.lock().unwrap();
            let mut pipeline_mut = pipeline.lock().unwrap();
            *pipeline_mut.fractal_mut().color_palette_mut() = palette.lock().unwrap().clone();
            pipeline_mut
                .fractal_mut()
                .set_image_specification(image_specification);
            let sampling_level = pipeline_mut.fractal().render_options().sampling_level;
            pipeline_mut.render(&mut color_image, sampling_level);
            last_sampling_level.store(sampling_level, Ordering::Release);
            render_task_is_busy.store(false, Ordering::Release);
            redraw_required.store(true, Ordering::Release);
        });
    }

    /// Spawn a background color-only re-render: sync the edited palette into
    /// the fractal and re-walk the existing field (no recompute). Reuses the
    /// last full render's `sampling_level` so it walks the same populated
    /// cells.
    fn recolorize(&mut self) {
        let display_buffer = self.display_buffer.clone();
        let pipeline = self.pipeline.clone();
        let palette = self.palette.clone();
        let render_task_is_busy = Arc::clone(&self.render_task_is_busy);
        let redraw_required = self.redraw_required.clone();
        let sampling_level = self.last_sampling_level.load(Ordering::Acquire);

        std::thread::spawn(move || {
            let mut color_image = display_buffer.lock().unwrap();
            let mut pipeline_mut = pipeline.lock().unwrap();
            *pipeline_mut.fractal_mut().color_palette_mut() = palette.lock().unwrap().clone();
            pipeline_mut.recolorize_only(&mut color_image, sampling_level);
            render_task_is_busy.store(false, Ordering::Release);
            redraw_required.store(true, Ordering::Release);
        });
    }
}
impl<F> RenderWindow for PixelGrid<F>
where
    F: Renderable + 'static,
{
    fn image_specification(&self) -> &ImageSpecification {
        self.view_control.image_specification()
    }

    fn reset(&mut self) {
        self.view_control.reset();
        self.adaptive_quality_regulator.reset();
        *self.palette.lock().unwrap() = self.initial_color_palette.clone();
        self.color_dirty.store(true, Ordering::Release);
    }

    fn update(
        &mut self,
        time: f64,
        center_command: CenterCommand,
        zoom_command: ZoomVelocityCommand,
    ) -> bool {
        // There are two flags to keep track of here, which are used to carefully sequence
        // the process of updating the window while keeping a fast main loop for tracking
        // keyboard and mouse events while doing the expensive render in the background.
        // The `render_required` flag indicates that the user updated the view port and the
        // fractal must be recomputed. The `redraw_required` indicates that the renderer has
        // updated the data in the `display_buffer` in the background and that it needs to
        // be copied to the screen using the `draw` method. The `render_task_is_busy` flag
        // is a lock that is used to ensure that we only attempt one render at a time, as
        // this task will use all available CPU resources.

        // There are two reasons that we might want to render the fractal:
        // (1) the view control reports that user-interaction has changed the view port onto the fractal
        // (2) the adaptive quality regulator reports that the render quality needs to be modified
        let user_interaction = self.view_control.update(time, center_command, zoom_command);

        // If a render completed, close out its timing *before* anything else —
        // including the save flow — can clear the flag or launch the next
        // render. Doing it here (rather than after the save match) ensures a
        // render that finishes just as a save begins is still recorded, so the
        // regulator's `render_start_time` is never left dangling across the
        // save. Capturing the period before launching also keeps it associated
        // with the correct command.
        let redraw_required = self.redraw_required.load(Ordering::Acquire);
        if redraw_required {
            self.adaptive_quality_regulator.finish_rendering(time);
        }

        // Gated Space-as-save flow. While active it takes over scheduling and
        // never *launches* a regulator-driven render, so the regulator's
        // mode/command stay frozen across the save: interaction resumes
        // afterward at its exact pre-save responsiveness (rather than the
        // roadmap's reset-and-cache, which is equivalent in effect). The
        // `finish_rendering` above still runs, but the save render bypasses
        // `begin_rendering`, so once the pre-save render is closed out it is a
        // no-op. `view_control` is also advanced above, so resuming never sees
        // a time jump.
        match self.save_state {
            SaveState::Pending => {
                // Wait for any in-flight render to finish, then launch a
                // forced full-quality save render. Forcing level 0.0 both
                // guarantees image fidelity *and* restores the user's
                // reference params, so the serialized params are full-quality
                // rather than whatever degraded state interaction left behind.
                if !self.render_task_is_busy.swap(true, Ordering::Acquire) {
                    self.redraw_required.store(false, Ordering::Release);
                    // The save render is a full render: it clones the current
                    // (possibly just-edited) palette into the fractal, so it
                    // already satisfies any pending color edit. Clear the flag
                    // so a redundant recolorize doesn't fire after the save
                    // (mirrors the full-render path below).
                    self.color_dirty.store(false, Ordering::Release);
                    self.pipeline
                        .lock()
                        .unwrap()
                        .fractal_mut()
                        .set_speed_optimization_level(0.0, &self.speed_optimizer_cache);
                    self.has_started_rendering = true;
                    self.render();
                    self.save_state = SaveState::Rendering;
                }
                return false;
            }
            SaveState::Rendering => {
                if redraw_required && !self.render_task_is_busy.load(Ordering::Acquire) {
                    self.write_snapshot();
                    self.save_state = SaveState::Idle;
                    // Report the completed full-quality frame so the app
                    // uploads it to the preview texture.
                    return true;
                }
                return false;
            }
            SaveState::Idle => {}
        }

        let render_required = self
            .adaptive_quality_regulator
            .render_required(user_interaction);
        let fallback_command = (user_interaction || !self.has_started_rendering).then_some(0.0);

        let mut launched_full_render = false;
        if let Some(command) = render_required.or(fallback_command) {
            // If we need to render, poll the render background thread to see if it is available...
            if !self.render_task_is_busy.swap(true, Ordering::Acquire) {
                // If we reach here, then the background thread is ready to render an image.
                self.pipeline
                    .lock()
                    .unwrap()
                    .fractal_mut()
                    .set_speed_optimization_level(command, &self.speed_optimizer_cache);
                // Mark the start of the render operation so that we can collect accurate timing.
                self.adaptive_quality_regulator
                    .begin_rendering(time, command);
                self.has_started_rendering = true;
                self.render();
                launched_full_render = true;
                // The full render clones the current (possibly just-edited)
                // palette into the fractal, so it already satisfies any
                // pending color edit. Clear the flag to avoid a redundant
                // recolorize firing the moment this render completes.
                self.color_dirty.store(false, Ordering::Release);
            }
        }

        // Color-only re-render after a palette edit. A full view render takes
        // priority — it regenerates the field a recolorize would re-walk — so
        // only recolorize when none was launched this tick and the worker is
        // free. If the worker is busy the dirty flag persists and we retry.
        if !launched_full_render
            && self.has_started_rendering
            && self.color_dirty.load(Ordering::Acquire)
            && !self.render_task_is_busy.swap(true, Ordering::Acquire)
        {
            self.color_dirty.store(false, Ordering::Release);
            self.recolorize();
        }

        // Redraw is required if a completed background render is waiting to be drawn.
        redraw_required
    }

    fn draw(&self, image: &mut ColorImage) {
        let [res_w, res_h] = self.image_specification().resolution;
        let width = res_w as usize;
        let height = res_h as usize;
        debug_assert_eq!(image.size, [width, height]);
        debug_assert_eq!(image.pixels.len(), width * height);
        let display_buffer = self.display_buffer.lock().unwrap();
        image.pixels.copy_from_slice(&display_buffer.pixels);
        self.redraw_required.store(false, Ordering::Release);
    }
}

// The previous `display_buffer_to_color_image_transposition` unit test was
// removed when the pipeline began writing directly into a row-major
// `ColorImage`; PixelGrid no longer transposes a column-major buffer. There
// isn't a useful seam to unit-test here today; the production behavior is
// covered by the CLI pixel-hash regression tests and the manual
// `cargo run -- explore` smoke tests.
