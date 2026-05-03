use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use egui::{Color32, ColorImage};

use crate::core::render_quality_fsm::AdaptiveOptimizationRegulator;

use super::{
    file_io::{FilePrefix, date_time_string, serialize_to_json_or_panic},
    image_utils::{
        ImageSpecification, Renderable, field_upsample_factor, write_image_to_file_or_panic,
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

    /// Saves the current rendered content to a file.
    ///
    /// This may also serialize additional data such as rendering parameters.
    fn render_to_file(&self);
}

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

    // Whether a render has ever been launched on this `PixelGrid`.
    has_started_rendering: bool,
}

const TARGET_RENDER_FRAMES_PER_SECOND: f64 = 24.0;

impl<F> PixelGrid<F>
where
    F: Renderable + Send + Sync + 'static,
{
    /// Construct a `PixelGrid` around the given fractal. Allocates the
    /// pipeline's reusable buffers at the user's full sampling level.
    pub fn new(time: f64, file_prefix: FilePrefix, view_control: ViewControl, renderer: F) -> Self {
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
            has_started_rendering: false,
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

    /// Spawn a background render on the pipeline. The pipeline's mutex
    /// serializes against the UI thread's parameter edits.
    fn render(&mut self) {
        let display_buffer = self.display_buffer.clone();
        let pipeline = self.pipeline.clone();
        let image_specification = *self.image_specification();
        let render_task_is_busy = Arc::clone(&self.render_task_is_busy);
        let redraw_required = self.redraw_required.clone();

        std::thread::spawn(move || {
            let mut color_image = display_buffer.lock().unwrap();
            let mut pipeline_mut = pipeline.lock().unwrap();
            pipeline_mut
                .fractal_mut()
                .set_image_specification(image_specification);
            let sampling_level = pipeline_mut.fractal().render_options().sampling_level;
            pipeline_mut.render(&mut color_image, sampling_level);
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

        // If redraw is required, it tells us that the previous rendering operation has
        // completed. Capture this timing *before* possibly launching a new render, so that
        // the measured period is associated with the correct command.
        let redraw_required = self.redraw_required.load(Ordering::Acquire);
        if redraw_required {
            self.adaptive_quality_regulator.finish_rendering(time);
        }

        let render_required = self
            .adaptive_quality_regulator
            .render_required(user_interaction);
        let fallback_command = (user_interaction || !self.has_started_rendering).then_some(0.0);

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
            }
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

    fn render_to_file(&self) {
        let datetime = date_time_string();

        serialize_to_json_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{datetime}.json")),
            &self.image_specification(),
        );

        let resolution = self.image_specification().resolution;
        let mut imgbuf = image::ImageBuffer::new(resolution[0], resolution[1]);
        {
            let display_buffer = self.display_buffer.lock().unwrap();
            let width = display_buffer.size[0];
            for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
                let c = display_buffer.pixels[(y as usize) * width + (x as usize)];
                *pixel = image::Rgb([c.r(), c.g(), c.b()]);
            }
        }

        write_image_to_file_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{datetime}.png")),
            |f| imgbuf.save(f),
        );
    }
}

// The previous `display_buffer_to_color_image_transposition` unit test was
// removed when the pipeline began writing directly into a row-major
// `ColorImage`; PixelGrid no longer transposes a column-major buffer. There
// isn't a useful seam to unit-test here today; the production behavior is
// covered by the CLI pixel-hash regression tests and the manual
// `cargo run -- explore` smoke tests.
