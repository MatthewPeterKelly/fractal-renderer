use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use image::Rgb;

use super::{
    controller::AdaptiveOptimizationRegulator,
    file_io::{date_time_string, serialize_to_json_or_panic, FilePrefix},
    image_utils::{create_buffer, write_image_to_file_or_panic, ImageSpecification, Renderable},
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

    /// Renders the internal buffer state to the screen. Typically `update()` would be called
    /// before `draw()`.
    ///
    /// # Parameters
    ///
    /// - `screen`: A mutable slice of `u8` representing the RGBA screen buffer where
    ///   color data for each pixel will be written.
    fn draw(&self, screen: &mut [u8]);

    /// Saves the current rendered content to a file.
    ///
    /// This may also serialize additional data such as rendering parameters.
    fn render_to_file(&self);
}

/// The `PixelGrid` is a generic implementation of the `RenderWindow`, which
/// supports all "solve by pixel" fractals. The key idea here is that we can
/// use generics to improve speed on the "per-pixel" calculations, but then
/// use runtime polymorphism (`dyn`) on the "once per image" updates for the
/// `explore` interface. This helps to keep the rendering pipeline efficient.
#[derive(Clone, Debug)]
pub struct PixelGrid<F: Renderable> {
    // The render will write into this buffer, which is locked with a mutex
    // during rendering. Once complete, it will be copied into the window
    // pixel-by-pixel in the `draw()` method.
    display_buffer: Arc<Mutex<Vec<Vec<Rgb<u8>>>>>,

    // Interprets the UI commands to pan and zoom, translating them into the image
    // specification used by the renderer.
    view_control: ViewControl,

    // Cache the file prefix so that we can use a consistent directory for writing
    // images to disk while exploring the fractal.
    file_prefix: FilePrefix,

    // Measures the render speed on each redraw, and then uses that to adaptively change
    // the trade-off between render quality and speed. Once the user stops interacting,
    // the quality will gradually increase, resulting in a progressively better render.
    // While interacting, this ensures that we have a fast response from the graphics.
    adaptive_quality_regulator: AdaptiveOptimizationRegulator,

    // Encapsulates all details required to render the image.
    // Wrapped in an `Arc<Mutex<>>` to enable render in a background thread.
    renderer: Arc<Mutex<F>>,

    // Cache used to enable dynamically adjusting parameters to hit frame per second target.
    speed_optimizer_cache: F::ReferenceCache,

    // Lock, used to ensure that we only run a single render background task.
    render_task_is_busy: Arc<AtomicBool>,

    // Set to `true` when rendering is complete and the display buffer needs
    // to be copied to the screen.
    redraw_required: Arc<AtomicBool>,
}

impl<F> PixelGrid<F>
where
    F: Renderable + Send + Sync + 'static,
{
    pub fn new(time: f64, file_prefix: FilePrefix, view_control: ViewControl, renderer: F) -> Self {
        let resolution = view_control.image_specification().resolution;
        let display_buffer = create_buffer(Rgb([0, 0, 0]), &resolution);
        let center_command = CenterCommand::Target(CenterTargetCommand {
            view_center: view_control.image_specification().center,
            pan_rate: 0.0,
        });

        let renderer = Arc::new(Mutex::new(renderer));

        // HACK -- render pipeline parameters
        let initial_render_command = 0.0;
        let target_update_period = 1.0 / 24.0;
        let max_command_delta = 0.05;

        let mut pixel_grid = Self {
            display_buffer: Arc::new(Mutex::new(display_buffer)),
            view_control,
            file_prefix,
            renderer: renderer.clone(),
            speed_optimizer_cache: renderer.lock().unwrap().reference_cache(),
            render_task_is_busy: Arc::new(AtomicBool::new(false)),
            redraw_required: Arc::new(AtomicBool::new(false)),
            adaptive_quality_regulator: AdaptiveOptimizationRegulator::new(
                initial_render_command,
                target_update_period,
                max_command_delta,
            ),
        };
        pixel_grid
            .view_control
            .update(time, center_command, ZoomVelocityCommand::zero());
        pixel_grid
    }

    /// Renders the fractal, pixel-by-pixel, on a background thread(s).
    fn render(&mut self) {
        let display_buffer = self.display_buffer.clone();
        let renderer = self.renderer.clone();
        let image_specification = *self.image_specification();
        let render_task_is_busy = Arc::clone(&self.render_task_is_busy);
        let redraw_required = self.redraw_required.clone();

        std::thread::spawn(move || {
            let mut display_buffer_mut = display_buffer.lock().unwrap();
            let mut renderer_mut = renderer.lock().unwrap();
            renderer_mut.set_image_specification(image_specification);
            renderer_mut.render_to_buffer(&mut display_buffer_mut);

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
        let render_required = self
            .adaptive_quality_regulator
            .render_required(user_interaction);
        if let Some(command) = render_required {
            if !self.render_task_is_busy.swap(true, Ordering::Acquire) {
                self.renderer
                    .lock()
                    .unwrap()
                    .set_speed_optimization_level(command, &self.speed_optimizer_cache);
                self.adaptive_quality_regulator
                    .begin_rendering(time, command);
                println!("Rendering now at level = {}...", command);
                self.render();
            }
        }
        let redraw_required = self.redraw_required.load(Ordering::Acquire);
        if redraw_required {
            self.adaptive_quality_regulator.finish_rendering(time);
        }
        redraw_required
    }

    fn draw(&self, screen: &mut [u8]) {
        debug_assert_eq!(
            screen.len(),
            (4 * self.image_specification().resolution[0]
                * self.image_specification().resolution[1]) as usize
        );
        let array_skip = self.image_specification().resolution[0] as usize;
        let display_buffer = self.display_buffer.lock().unwrap();
        for (flat_index, pixel) in screen.chunks_exact_mut(4).enumerate() {
            let j = flat_index / array_skip;
            let i = flat_index % array_skip;
            let raw_pixel = display_buffer[i][j];
            let color = [raw_pixel[0], raw_pixel[1], raw_pixel[2], 255];
            pixel.copy_from_slice(&color);
        }
        self.redraw_required.store(false, Ordering::Release);
    }

    fn render_to_file(&self) {
        let datetime = date_time_string();

        serialize_to_json_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{datetime}.json")),
            &self.image_specification(),
        );

        let mut imgbuf = image::ImageBuffer::new(
            self.image_specification().resolution[0],
            self.image_specification().resolution[1],
        );

        {
            let display_buffer = self.display_buffer.lock().unwrap();
            for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
                *pixel = display_buffer[x as usize][y as usize];
            }
        }

        write_image_to_file_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{datetime}.png")),
            |f| imgbuf.save(f),
        );
    }
}
