use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use image::Rgb;

use super::{
    file_io::{date_time_string, serialize_to_json_or_panic, FilePrefix},
    image_utils::{create_buffer, write_image_to_file_or_panic, ImageSpecification, Renderable},
    view_control::{CenterCommand, ViewControl, ZoomVelocityCommand},
};

/// A trait for managing and rendering a graphical view with controls for recentering,
/// panning, zooming, updating, and saving the rendered output. This is the core interface
/// used by the "explore" GUI to interact with the different fractals.
pub trait RenderWindow {
    /// Provides access to the current image specification for the window
    fn image_specification(&self) -> &ImageSpecification;

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
    display_buffer: Arc<Mutex<Vec<Vec<Rgb<u8>>>>>,
    view_control: ViewControl,
    file_prefix: FilePrefix,
    renderer: Arc<Mutex<F>>,
    render_task_is_busy: Arc<AtomicBool>,
    redraw_required: Arc<AtomicBool>,
}

impl<F> PixelGrid<F>
where
    F: Renderable + Send + Sync + 'static,
{
    pub fn new(time: f64, file_prefix: FilePrefix, view_control: ViewControl, renderer: F) -> Self {
        let resolution = view_control.image_specification().resolution;
        let display_buffer = create_buffer(Rgb([0, 0, 0]), &resolution);

        let mut pixel_grid = Self {
            display_buffer: Arc::new(Mutex::new(display_buffer)),
            view_control,
            file_prefix,
            renderer: Arc::new(Mutex::new(renderer)), // Wrap renderer in Arc<Mutex>
            render_task_is_busy: Arc::new(AtomicBool::new(false)),
            redraw_required: Arc::new(AtomicBool::new(false)),
        };
        pixel_grid.update(time, CenterCommand::Idle(), ZoomVelocityCommand::zero());
        pixel_grid
    }
}
impl<F> RenderWindow for PixelGrid<F>
where
    F: Renderable + 'static,
{
    fn image_specification(&self) -> &ImageSpecification {
        self.view_control.image_specification()
    }

    // TODO: consider a "fast update" that solves 2x2 or 3x3 pixel blocks while moving?
    fn update(
        &mut self,
        time: f64,
        center_command: CenterCommand,
        zoom_command: ZoomVelocityCommand,
    ) -> bool {
        self.view_control.update(time, center_command, zoom_command);

        let update_required = true;

        if update_required {
            // .swap() here returns previous value and atomically sets value to true.
            if !self.render_task_is_busy.swap(true, Ordering::Acquire) {
                let display_buffer = self.display_buffer.clone();
                let renderer = self.renderer.clone();
                let image_specification = self.image_specification().clone();
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

        self.redraw_required.load(Ordering::Acquire)
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
                .full_path_with_suffix(&format!("_{}.json", datetime)),
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
                .full_path_with_suffix(&format!("_{}.png", datetime)),
            |f| imgbuf.save(f),
        );
    }
}
