use egui::{Color32, ColorImage};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::{
    io::{self, Write},
    path::PathBuf,
};

use crate::core::color_map::ColorMapKind;
use crate::core::histogram::{CumulativeDistributionFunction, Histogram};
use crate::core::interpolation::Interpolator;
use crate::core::render_pipeline::RenderingPipeline;

use super::file_io::{FilePrefix, serialize_to_json_or_panic};
use super::stopwatch::Stopwatch;

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ImageSpecification {
    pub resolution: [u32; 2],
    pub center: [f64; 2],
    pub width: f64,
}

/**
 * Used to fully-specify both an image resolution and how it is anchored into the "real"
 * space in which the fractal (or other subject) lives. The height in "real" space is derived
 * from the aspect ratio of the image and the specified width.
 */
impl ImageSpecification {
    pub fn height(&self) -> f64 {
        self.width * (self.resolution[1] as f64) / (self.resolution[0] as f64)
    }

    /// Returns a new image specification object with the same center and
    /// width, but with the resolution scaled by `subpixel_count`. Used by
    /// `chaos_game` for its anti-aliasing mask.
    pub fn upsample(&self, subpixel_count: u32) -> ImageSpecification {
        assert!(subpixel_count > 0);
        ImageSpecification {
            resolution: [
                self.resolution[0] * subpixel_count,
                self.resolution[1] * subpixel_count,
            ],
            center: self.center,
            width: self.width,
        }
    }

    /**
     * Returns a new image specification object with the same center and width, but
     * with a resolution scaled to approximately hit the target number of pixels.
     * Implemented by rescaling the resolution of each axis and rounding up to the nearest
     * integer.
     *
     * @param: target pixel count in the new image, lower bound.
     */
    pub fn scale_to_total_pixel_count(&self, target_pixel_count: u32) -> ImageSpecification {
        assert!(target_pixel_count > 0);
        let old_pixel_count = self.resolution[0] * self.resolution[1];
        let scale = ((target_pixel_count as f64) / (old_pixel_count as f64)).sqrt();
        ImageSpecification {
            resolution: [
                (self.resolution[0] as f64 * scale).ceil() as u32,
                (self.resolution[1] as f64 * scale).ceil() as u32,
            ],
            center: self.center,
            width: self.width,
        }
    }
}

/**
 * Describes a rectangular region in space.
 */
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ViewRectangle {
    pub center: [f64; 2],
    pub dimensions: [f64; 2],
}

impl ViewRectangle {
    /// Given a vector of 2D points, compute the smallest view rectangle
    /// that will contain all points.
    pub fn from_vertices(vertices: &[[f64; 2]]) -> ViewRectangle {
        assert!(!vertices.is_empty());

        let find_center_and_range = |dim| {
            let mut min_val: f64 = vertices[0][dim];
            let mut max_val = min_val;

            for &vertex in &vertices[1..] {
                min_val = min_val.min(vertex[dim]);
                max_val = max_val.max(vertex[dim]);
            }

            let center = 0.5 * (min_val + max_val);
            let range = max_val - min_val;
            (center, range)
        };

        let (center_x, range_x) = find_center_and_range(0);
        let (center_y, range_y) = find_center_and_range(1);

        ViewRectangle {
            center: [center_x, center_y],
            dimensions: [range_x, range_y],
        }
    }
}

/// Allows a set of parameters to be dynamically adjusted to hit a target frame rate.
/// The `ReferenceCache` is used to store a reference sub-set of parameters.
pub trait SpeedOptimizer {
    type ReferenceCache;

    /// Constructs a reference cache representing the current state of the parameters.
    fn reference_cache(&self) -> Self::ReferenceCache;

    /// Modifies the parameters of the image in-place.
    /// An optimization level of...
    /// -- 0.0 --> render with user-specified parameters (stored in `cache`).
    /// -- 1.0 --> render as fast as possible (with dramatic loss of image quality).
    ///
    /// It is up to each fractal implementation to anchor the upper bound to a meaningful value.
    ///
    /// Note: parameters modified by this call should strictly reduce the render time, and
    /// should not change the size of the image or underlying data structures.
    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache);
}

/// Scales down a parameter based on a the speed optimization level factor.
/// The logic is slightly complicated to handle the case where the user-defined
///  `cached_value` is already below the `lower_bound`.
///
/// Assumes that `level` is in [0,1] (from the `SpeedOptimizer`), using the convention:
///   0 --> cached_value (high quality)
///   1 --> "as fast as possible".   (hit lower bound)
///
/// This should be used for parameters where smaller values correspond to higher speed.
///
/// The `interpolator` parameter is used to specify how the scaling should be performed,
/// typically either linearly (ClampedLinearInterpolator) or logarithmically (ClampedLogInterpolator).
pub fn scale_down_parameter_for_speed<I>(
    lower_bound: f64,
    cached_value: f64,
    level: f64,
    interpolator: I,
) -> f64
where
    I: Interpolator<f64, f64>,
{
    if cached_value < lower_bound {
        return cached_value;
    }
    interpolator.interpolate(level, cached_value, lower_bound)
}

/// Scales up a parameter based on a the speed optimization level factor.
/// The logic is slightly complicated to handle the case where the user-defined
///  `cached_value` is already above the `upper_bound`.
///
/// Assumes that `level` is in [0,1] (from the `SpeedOptimizer`), using the convention:
///   0 --> cached_value (high quality)
///   1 --> "as fast as possible".  (hit upper bound)
///
/// This should be used for parameters where smaller values correspond to higher speed.
///
/// The `interpolator` parameter is used to specify how the scaling should be performed,
/// typically either linearly (ClampedLinearInterpolator) or logarithmically (ClampedLogInterpolator).
pub fn scale_up_parameter_for_speed<I>(
    upper_bound: f64,
    cached_value: f64,
    level: f64,
    interpolator: I,
) -> f64
where
    I: Interpolator<f64, f64>,
{
    if cached_value > upper_bound {
        return cached_value;
    }
    interpolator.interpolate(level, cached_value, upper_bound)
}

/// Parameters shared by multiple fractal types that control how the fractal
/// is rendered to the screen.
///
/// `sampling_level` collapses the legacy `(subpixel_antialiasing,
/// downsample_stride)` axes into a single signed integer:
/// - `+n` (`n > 0`): anti-aliasing at `(n+1)²` samples per output pixel.
/// - `0`: baseline (one sample per output pixel, no averaging).
/// - `−n` (`n > 0`): block-fill, one sample per `(n+1)²` output pixels.
///
/// The JSON value is the **maximum** the pipeline ever runs at — the field
/// buffer is sized to accommodate it. The adaptive regulator drives the
/// runtime value passed to `RenderingPipeline::render`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct RenderOptions {
    /// User-facing sampling level (see struct docs). `0` is baseline.
    pub sampling_level: i32,
}

/// Most extreme block-fill the regulator pushes to under load
/// (`-7` ↔ 8×8 block-fill). Mirrors the legacy `MAX_DOWNSAMPLE_STRIDE`.
const MIN_RUNTIME_SAMPLING_LEVEL: i32 = -7;

impl SpeedOptimizer for RenderOptions {
    type ReferenceCache = RenderOptions;

    fn reference_cache(&self) -> Self::ReferenceCache {
        *self
    }

    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache) {
        // Three-piece curve (chosen to match the *spirit* of the legacy
        // two-axis behavior — AA drops fast, downsample activates slow):
        // 1. `[0, 0.2]`: AA quality drops from cached → 0 (baseline).
        // 2. `[0.2, 0.5]`: hold at baseline.
        // 3. `[0.5, 1.0]`: block-fill ramps from baseline (or cached, if
        //    cached is already negative) toward `MIN_RUNTIME_SAMPLING_LEVEL`.
        let cached = cache.sampling_level as f64;
        let runtime = if cached > 0.0 {
            if level <= 0.2 {
                cached * (1.0 - (level / 0.2))
            } else if level <= 0.5 {
                0.0
            } else {
                (MIN_RUNTIME_SAMPLING_LEVEL as f64) * ((level - 0.5) / 0.5)
            }
        } else if cached <= MIN_RUNTIME_SAMPLING_LEVEL as f64 {
            // User already at or past the floor; nothing more to drop.
            cached
        } else {
            cached + (MIN_RUNTIME_SAMPLING_LEVEL as f64 - cached) * level
        };
        self.sampling_level = runtime.round() as i32;
    }
}

/// Drives the new four-phase `RenderingPipeline`. Per-(sub)pixel dispatch
/// is fully monomorphized through `Self::ColorMap: ColorMapKind`.
pub trait Renderable: Sync + Send + SpeedOptimizer {
    /// The type of parameters that describe the renderable object.
    type Params: Serialize + Debug;

    /// Statically-paired color-map shape and per-(sub)pixel cell type. Drives
    /// the field buffer's element type and the colorize hot path.
    type ColorMap: ColorMapKind;

    /// Access the current image specification for the renderable object.
    fn image_specification(&self) -> &ImageSpecification;

    /// Access to the rendering options.
    fn render_options(&self) -> &RenderOptions;

    /// Set the image specification for the renderable object. May trigger
    /// recomputation of dependent state in the impl.
    fn set_image_specification(&mut self, image_specification: ImageSpecification);

    /// Write diagnostics information, typically to a log file, for the
    /// renderable object. This might include parameters or a histogram
    /// summary.
    fn write_diagnostics<W: Write>(&self, writer: &mut W) -> io::Result<()>;

    /// @return a reference to the internal parameters of the renderable
    /// object, which can then be serialized to a JSON file.
    fn params(&self) -> &Self::Params;

    /// Histogram capacity in bins; used by the pipeline to allocate the
    /// `Histogram` once at construction. Fractals that never normalize
    /// (e.g. DDP) can return any positive value.
    fn histogram_bin_count(&self) -> usize;

    /// Maximum value the pipeline expects to insert into the histogram.
    /// Used to size each bin's range. Fractals that never normalize can
    /// return any positive number.
    fn histogram_max_value(&self) -> f32;

    /// Number of entries in each precomputed color lookup table held by
    /// the colorize cache. Variants without lookup tables (e.g.
    /// `ForegroundBackground`) can return any value.
    fn lookup_table_count(&self) -> usize;

    /// (a) Fill the preallocated `field` buffer with raw, un-normalized
    /// values. The cells the impl populates depend on `sampling_level`
    /// and `n_max_plus_1` (derived from `field.len() / W`).
    fn compute_raw_field(
        &self,
        sampling_level: i32,
        field: &mut Vec<Vec<<Self::ColorMap as ColorMapKind>::Cell>>,
    );

    /// (b) Walk the populated cells of `field` and insert each value into
    /// `histogram`. The pipeline calls `histogram.reset()` first; impls
    /// should not reset. Default impl is a no-op (DDP).
    fn populate_histogram(
        &self,
        _sampling_level: i32,
        _field: &[Vec<<Self::ColorMap as ColorMapKind>::Cell>],
        _histogram: &Histogram,
    ) {
    }

    /// (c) Replace each populated cell's raw value with its CDF percentile,
    /// in place. Default impl is a no-op (DDP).
    fn normalize_field(
        &self,
        _sampling_level: i32,
        _cdf: &CumulativeDistributionFunction,
        _field: &mut Vec<Vec<<Self::ColorMap as ColorMapKind>::Cell>>,
    ) {
    }

    /// Reference to the concrete color-map data driving colorization.
    fn color_map(&self) -> &Self::ColorMap;
}

/// Render a fractal to a PNG file (and a sibling JSON / diagnostics file).
/// Drives the new `RenderingPipeline` at the user's full sampling level.
pub fn render<T: Renderable + 'static>(
    renderable: T,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch = Stopwatch::new("Render Stopwatch".to_owned());

    let spec = *renderable.image_specification();
    let cached_sampling_level = renderable.render_options().sampling_level;
    let n_max_plus_1 = field_upsample_factor(cached_sampling_level);
    let histogram_bin_count = renderable.histogram_bin_count();
    let histogram_max_value = renderable.histogram_max_value();
    let lookup_table_count = renderable.lookup_table_count();

    serialize_to_json_or_panic(
        file_prefix.full_path_with_suffix(".json"),
        renderable.params(),
    );
    stopwatch.record_split("basic setup".to_owned());

    let mut pipeline = RenderingPipeline::new(
        renderable,
        n_max_plus_1,
        histogram_bin_count,
        histogram_max_value,
        lookup_table_count,
    );
    let mut color_image = ColorImage::filled(
        [spec.resolution[0] as usize, spec.resolution[1] as usize],
        Color32::BLACK,
    );
    pipeline.render(&mut color_image, cached_sampling_level);
    stopwatch.record_split("render pipeline".to_owned());

    let mut imgbuf = image::ImageBuffer::new(spec.resolution[0], spec.resolution[1]);
    let width = color_image.size[0];
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let c = color_image.pixels[(y as usize) * width + (x as usize)];
        *pixel = image::Rgb([c.r(), c.g(), c.b()]);
    }
    stopwatch.record_split("copy into image buffer".to_owned());
    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });
    stopwatch.record_split("write PNG".to_owned());

    let mut diagnostics_file = file_prefix.create_file_with_suffix("_diagnostics.txt");
    stopwatch.display(&mut diagnostics_file)?;
    pipeline
        .fractal()
        .write_diagnostics(&mut diagnostics_file)?;

    Ok(())
}

/// Compute the `n_max_plus_1` upsample factor for the field buffer based on
/// the user's cached sampling level.
///
/// - Positive cached → `cached + 1` (allocates the AA subpixel grid).
/// - Zero or negative cached → `1` (no AA grid; block-fill is sparse over
///   a 1× field).
pub fn field_upsample_factor(cached_sampling_level: i32) -> usize {
    if cached_sampling_level > 0 {
        (cached_sampling_level as usize) + 1
    } else {
        1
    }
}

/**
 * Allows the user to specify only the resolution of the image and how much "extra space" to leave
 * around the fractal (subject) in the image. The real coordinates are derived automatically from
 * other information in the fractal.
 *
 * The `FitImage` can then be converted into a full `ImageSpecification` to be passed into other code.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FitImage {
    pub resolution: [u32; 2],
    pub padding_scale: f64,
}

impl FitImage {
    pub fn image_specification(&self, view_rectangle: &ViewRectangle) -> ImageSpecification {
        let pixel_height = self.resolution[1] as f64;
        let pixel_width = self.resolution[0] as f64;
        let dims_height = view_rectangle.dimensions[1];
        let dims_width = view_rectangle.dimensions[0];

        let aspect_ratio = pixel_height / pixel_width; // of the rendered image
        let selected_width = if aspect_ratio > (dims_height / dims_width) {
            dims_width
        } else {
            dims_height / aspect_ratio
        };

        ImageSpecification {
            resolution: self.resolution,
            center: view_rectangle.center,
            width: self.padding_scale * selected_width,
        }
    }
}

#[derive(Clone, Debug)]
/**
 * Used to map from image space into the "regular" domain used to generate the fractals.
 */
pub struct LinearPixelMap {
    offset: f64,
    slope: f64,
}

impl LinearPixelMap {
    /**
     * @param n: number of pixels spanned by [x0,x1]
     * @param x0: output of the map at 0
     * @param x1: output of the map at n-1
     */
    pub fn new(n: u32, x0: f64, x1: f64) -> LinearPixelMap {
        assert!(n > 0);
        let offset = x0;
        let slope = (x1 - x0) / ((n - 1) as f64);
        LinearPixelMap { offset, slope }
    }

    pub fn new_from_center_and_width(n: u32, center: f64, width: f64) -> LinearPixelMap {
        LinearPixelMap::new(n, center - 0.5 * width, center + 0.5 * width)
    }

    // Map from pixel (integer) to point (float)
    pub fn map(&self, index: u32) -> f64 {
        self.offset + self.slope * (index as f64)
    }

    // Maps from point to pixel.
    // Rename as part of https://github.com/MatthewPeterKelly/fractal-renderer/issues/48?
    pub fn inverse_map(&self, point: f64) -> u32 {
        ((point - self.offset) / self.slope) as u32
    }
}

#[derive(Clone, Debug)]
pub struct PixelMapper {
    pub width: LinearPixelMap,
    pub height: LinearPixelMap,
}

// TODO:  standardize on "point" = Vector2 and "pixel_coordinate" = (u32, u32)?
// https://github.com/MatthewPeterKelly/fractal-renderer/issues/47
// Improve this class generally:
// https://github.com/MatthewPeterKelly/fractal-renderer/issues/48
impl PixelMapper {
    pub fn new(image_specification: &ImageSpecification) -> PixelMapper {
        PixelMapper {
            width: LinearPixelMap::new_from_center_and_width(
                image_specification.resolution[0],
                image_specification.center[0],
                image_specification.width,
            ),
            height: LinearPixelMap::new_from_center_and_width(
                image_specification.resolution[1],
                image_specification.center[1],
                -image_specification.height(),
            ),
        }
    }

    pub fn inverse_map(&self, point: &[f64; 2]) -> [u32; 2] {
        [
            self.width.inverse_map(point[0]),
            self.height.inverse_map(point[1]),
        ]
    }

    pub fn map(&self, point: &(u32, u32)) -> (f64, f64) {
        let (x, y) = point;
        (self.width.map(*x), self.height.map(*y))
    }
}

/// Coordinate of a subpixel within the entire image. Used by `chaos_game`.
pub struct SubpixelIndex {
    /// Output pixel containing the sample.
    pub pixel: [u32; 2],
    /// Subpixel offset within that pixel.
    pub subpixel: [u32; 2],
}

/// Splits a query into a pixel index and a subpixel index. Used by
/// `chaos_game` for its antialiasing mask.
pub struct UpsampledPixelMapper {
    pixel_mapper: PixelMapper,
    subpixel_count: u32,
}

impl UpsampledPixelMapper {
    /// Construct an upsampled pixel mapper.
    pub fn new(
        image_specification: &ImageSpecification,
        subpixel_count: u32,
    ) -> UpsampledPixelMapper {
        UpsampledPixelMapper {
            pixel_mapper: PixelMapper::new(&image_specification.upsample(subpixel_count)),
            subpixel_count,
        }
    }

    /// Map a point in fractal space to a pixel + subpixel index.
    pub fn inverse_map(&self, point: &[f64; 2]) -> SubpixelIndex {
        let [x_raw, y_raw] = self.pixel_mapper.inverse_map(point);
        SubpixelIndex {
            pixel: [x_raw / self.subpixel_count, y_raw / self.subpixel_count],
            subpixel: [x_raw % self.subpixel_count, y_raw % self.subpixel_count],
        }
    }
}

/// Bitmask for a square subpixel grid (max 8 per side). Used by `chaos_game`
/// to track which subpixels of an output pixel were hit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SubpixelGridMask {
    bitmask: u64,
}

impl SubpixelGridMask {
    /// Empty mask.
    pub fn new() -> SubpixelGridMask {
        SubpixelGridMask { bitmask: 0 }
    }

    /// Mark subpixel `coordinate` (in `[0, count_per_side)`²) as hit.
    pub fn insert(&mut self, count_per_side: u32, coordinate: [u32; 2]) {
        let [x, y] = coordinate;
        assert!(x < count_per_side);
        assert!(y < count_per_side);
        let index = x * count_per_side + y;
        self.bitmask |= 1 << index;
    }

    /// Number of subpixels marked.
    pub fn count_ones(&self) -> u32 {
        self.bitmask.count_ones()
    }
}

pub fn write_image_to_file_or_panic<F, T, E>(filename: std::path::PathBuf, save_lambda: F)
where
    F: FnOnce(&PathBuf) -> Result<T, E>,
{
    save_lambda(&filename)
        .unwrap_or_else(|_| panic!("ERROR:  Unable to write image file: {}", filename.display()));
    println!("INFO:  Wrote image file to: {}", filename.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::interpolation::{ClampedLinearInterpolator, ClampedLogInterpolator};
    use approx::assert_relative_eq;

    #[test]
    fn test_view_port_from_vertices() {
        let vertices = vec![[1.0, 2.0], [3.0, 5.0], [-1.0, -2.0], [2.0, 3.0]];

        let view_rectangle = ViewRectangle::from_vertices(&vertices);

        assert_eq!(view_rectangle.center, [1.0, 1.5]);
        assert_eq!(view_rectangle.dimensions, [4.0, 7.0]);
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn test_view_port_empty_vertices() {
        let vertices: Vec<[f64; 2]> = Vec::new();
        ViewRectangle::from_vertices(&vertices);
    }

    #[test]
    fn test_image_specification_height() {
        let image_specification = ImageSpecification {
            resolution: [5, 23],
            center: [2.6, 3.4],
            width: 8.5,
        };

        // The `height` is defined S.T. that aspect ratio is identical in both the image and the regular space.
        let aspect_ratio = image_specification.width / image_specification.height();
        let pixel_aspect_ratio =
            (image_specification.resolution[0] as f64) / (image_specification.resolution[1] as f64);
        assert_eq!(aspect_ratio, pixel_aspect_ratio);
    }

    #[test]
    fn test_linear_pixel_map_domain_bounds_pos() {
        let n = 7;
        let x0 = 1.23;
        let x1 = 56.2;

        let pixel_map = LinearPixelMap::new(n, x0, x1);

        let tol = 1e-6;
        assert_relative_eq!(pixel_map.map(0), x0, epsilon = tol);
        assert_relative_eq!(pixel_map.map(n - 1), x1, epsilon = tol);
    }

    #[test]
    fn test_linear_pixel_map_domain_bounds_neg() {
        let n = 11;
        let x0 = 1.23;
        let x1 = -0.05;

        let pixel_map = LinearPixelMap::new(n, x0, x1);

        let tol = 1e-6;
        assert_relative_eq!(pixel_map.map(0), x0, epsilon = tol);
        assert_relative_eq!(pixel_map.map(n - 1), x1, epsilon = tol);
    }

    #[test]
    fn test_scale_to_total_pixel_count() {
        let image_spec = ImageSpecification {
            resolution: [800, 600],
            center: [0.0, 0.0],
            width: 1.0,
        };

        let scaled_spec = image_spec.scale_to_total_pixel_count(32);
        assert_eq!(scaled_spec.center, image_spec.center);
        assert_eq!(scaled_spec.width, image_spec.width);
        assert_eq!(scaled_spec.resolution, [7, 5]);
    }

    #[test]
    fn test_scale_parameter_for_speed_interpolator_and_guard_logic() {
        let lin = ClampedLinearInterpolator;
        let log = ClampedLogInterpolator;

        // Check the case when the user-defined cache value is outside of the bound,
        // so we expect it to be returned directly.
        let cached_value = 1e-8;
        let lower_bound = 1e-6;
        assert_relative_eq!(
            scale_down_parameter_for_speed(lower_bound, cached_value, 0.0, lin),
            cached_value,
            epsilon = 0.0
        );
        assert_relative_eq!(
            scale_down_parameter_for_speed(lower_bound, cached_value, 1.0, log),
            cached_value,
            epsilon = 0.0
        );

        // Same thing, but for scaling up:
        let upper_bound = 1e-6;
        let cached_value = 1e-1;
        assert_relative_eq!(
            scale_up_parameter_for_speed(upper_bound, cached_value, 0.0, lin),
            cached_value,
            epsilon = 0.0
        );
        assert_relative_eq!(
            scale_up_parameter_for_speed(upper_bound, cached_value, 1.0, log),
            cached_value,
            epsilon = 0.0
        );

        // Check endpoints on all four variations:
        let lower = 1e-6;
        let upper = 1e-2;
        let cached = 1e-2; // >= lower, so down-scaling uses interpolator
        assert_relative_eq!(
            scale_down_parameter_for_speed(lower, cached, 0.0, lin),
            cached,
            epsilon = 0.0
        );
        assert_relative_eq!(
            scale_down_parameter_for_speed(lower, cached, 1.0, log),
            lower,
            epsilon = 0.0
        );

        let cached = 1e-6; // <= upper, so up-scaling uses interpolator
        assert_relative_eq!(
            scale_up_parameter_for_speed(upper, cached, 0.0, lin),
            cached,
            epsilon = 0.0
        );
        assert_relative_eq!(
            scale_up_parameter_for_speed(upper, cached, 1.0, log),
            upper,
            epsilon = 0.0
        );

        // --- sanity: different interpolators produce different midpoints (ensures wiring) ---
        // Down: cached=1e-2 to lower=1e-6 at level=0.5:
        // linear midpoint ~ 0.0050005, log midpoint = 1e-4
        let cached = 1e-2;
        let down_lin = scale_down_parameter_for_speed(lower, cached, 0.5, lin);
        let down_log = scale_down_parameter_for_speed(lower, cached, 0.5, log);
        assert_relative_eq!(down_log, 1e-4, epsilon = 1e-15);
        assert_relative_eq!(down_lin, 0.5 * (cached + lower), epsilon = 1e-15);
        assert!(down_lin > down_log);

        // Up: cached=1e-6 to upper=1e-2 at level=0.5:
        // linear midpoint ~ 0.0050005, log midpoint = 1e-4
        let cached = 1e-6;
        let up_lin = scale_up_parameter_for_speed(upper, cached, 0.5, lin);
        let up_log = scale_up_parameter_for_speed(upper, cached, 0.5, log);
        assert_relative_eq!(up_log, 1e-4, epsilon = 1e-15);
        assert_relative_eq!(up_lin, 0.5 * (cached + upper), epsilon = 1e-15);
        assert!(up_lin > up_log);
    }
}
