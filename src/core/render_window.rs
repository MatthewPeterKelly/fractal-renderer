use image::Rgb;
use nalgebra::Vector2;

use super::{file_io::FilePrefix, image_utils::{create_buffer, ImageSpecification, PointRenderFn}};

/// A trait for managing and rendering a graphical view with controls for recentering,
/// panning, zooming, updating, and saving the rendered output. This is the core interface
/// used by the "explore" GUI to interact with the different fractals.
pub trait RenderWindow {
    /// Recenters the view to a specific point in the 2D space.
    ///
    /// # Parameters
    ///
    /// - `center`: A reference to a `Vector2<f64>` specifying the new center coordinates.
    fn recenter(&mut self, center: &Vector2<f64>);

    /// Pans the view by a specified fraction of the view's current size.
    ///
    /// # Parameters
    ///
    /// - `view_fraction`: A `Vector2<f32>`, normalized by the current window size.
    ///   For example, passing [1,0] would move the image center by exactly one window width.
    fn pan_view(&mut self, view_fraction: &Vector2<f32>);

    /// Zooms the view by a given scaling factor.
    ///
    /// # Parameters
    ///
    /// - `scale`: A `f32`, representing the ratio of the desired to current window width.
    fn zoom(&mut self, scale: f32);

    /// Recompute the entire fractal if any internal parameters have changed. This should be
    /// a no-op if called with no internal changes.
    fn update(&mut self);

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





#[derive(Clone, Debug)]
struct PixelGrid {
    display_buffer: Vec<Vec<Rgb<u8>>>, // rendered to the screen on `draw()`
    scratch_buffer: Vec<Vec<Rgb<u8>>>, // updated in-place on `update()`
    image_specification: ImageSpecification,
    update_required: bool, // used to mark when the image_specification has changed.
    file_prefix: FilePrefix, // used for writing intermediate image frames to file
}

/**
 * I think an answer here is to cache the renderer object into the pixel grid, making that
 * type explicit. Then dynamic dispatch on the call to update, rather on the call to the
 * renderer itself.
 */

impl PixelGrid {
    fn new<F: PointRenderFn>(
        file_prefix: FilePrefix,
        image_specification: ImageSpecification,
        pixel_renderer: F,
    ) -> Self {
        let mut grid = Self {
            display_buffer: create_buffer(Rgb([0, 0, 0]), &image_specification.resolution),
            scratch_buffer: create_buffer(Rgb([0, 0, 0]), &image_specification.resolution),
            image_specification,
            update_required: true,
            file_prefix,
        };
        grid.update(&pixel_renderer);
        grid
    }

}
impl RenderWindow for PixelGrid {
    fn recenter(&mut self, center: &nalgebra::Vector2<f64>) {
        self.image_specification.center = *center;
        self.update_required = true;
    }

    fn pan_view(&mut self, view_fraction: &nalgebra::Vector2<f32>) {
        let x_delta = view_fraction[0] as f64 * self.image_specification.width;
        let y_delta = view_fraction[1] as f64 * self.image_specification.height();
        self.recenter(&nalgebra::Vector2::new(
            self.image_specification.center[0] + x_delta,
            self.image_specification.center[1] + y_delta,
        ));
    }

    fn zoom(&mut self, scale: f32) {
        self.image_specification.width *= scale as f64;
        self.update_required = true;
    }

    /**
     *  Computes the fractal; stored in a double buffer.
     */
    fn update<F>(&mut self, pixel_renderer: F)
    where
        F: PointRenderFn,
    {
        generate_scalar_image_in_place(
            &self.image_specification,
            pixel_renderer,
            &mut self.scratch_buffer,
        );
        std::mem::swap(&mut self.scratch_buffer, &mut self.display_buffer);
        self.update_required = false;
    }

    /**
     *  Renders data from the double buffer to the screen.
     */
    fn draw(&self, screen: &mut [u8]) {
        // The screen buffer should be 4x the size of our buffer because it has RGBA channels
        // where as we only have a scalar channel that is mapped through a color map.
        debug_assert_eq!(
            screen.len(),
            (4 * self.image_specification.resolution[0] * self.image_specification.resolution[1])
                as usize
        );
        let array_skip = self.image_specification.resolution[0] as usize;
        for (flat_index, pixel) in screen.chunks_exact_mut(4).enumerate() {
            let j = flat_index / array_skip;
            let i = flat_index % array_skip;
            let raw_pixel = self.display_buffer[i][j];
            let color = [raw_pixel[0], raw_pixel[1], raw_pixel[2], 255];
            pixel.copy_from_slice(&color);
        }
    }

    fn render_to_file(&self) {
        let datetime = date_time_string();

        // TODO:  eventually generalize this to write the entire parameter struct:
        // https://github.com/MatthewPeterKelly/fractal-renderer/issues/68
        serialize_to_json_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{}.json", datetime)),
            &self.image_specification,
        );

        // Save the image to a file, deducing the type from the file name
        // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
        let mut imgbuf = image::ImageBuffer::new(
            self.image_specification.resolution[0],
            self.image_specification.resolution[1],
        );

        // Iterate over the coordinates and pixels of the image
        for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
            *pixel = self.display_buffer[x as usize][y as usize];
        }

        write_image_to_file_or_panic(
            self.file_prefix
                .full_path_with_suffix(&format!("_{}.png", datetime)),
            |f| imgbuf.save(f),
        );
    }
}
