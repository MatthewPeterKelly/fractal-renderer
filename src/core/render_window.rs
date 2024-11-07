use nalgebra::Vector2;

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