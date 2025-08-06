use image::Rgb;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::{
    io::{self, Write},
    path::PathBuf,
};

use super::file_io::{serialize_to_json_or_panic, FilePrefix};
use super::stopwatch::Stopwatch;

/// Linear interpolation between two points, with extrapolation:
///
/// alpha = 0   --->  low
/// alpha = 1   --->  upp
///
/// TODO:  put this in some math utility library?
/// Yep:  just use LinearInterpolator::interppolate()
pub fn interpolate(low: f64, upp: f64, alpha: f64) -> f64 {
    upp * alpha + (1.0 - alpha) * low
}

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

    /**
     * Used for anti-aliasing the image calculations. Computes a vector of offsets to be
     * applied within a single pixel, generating a dense grid of samples within that pixel.
     */
    pub fn subpixel_offset_vector(&self, subpixel_antialiasing: u32) -> Vec<[f64; 2]> {
        let n = subpixel_antialiasing + 1;
        let mut offsets = Vec::with_capacity((n * n) as usize);
        let step = 1.0 / n as f64;

        let pixel_width = self.width / (self.resolution[0] as f64);
        let pixel_height = self.height() / (self.resolution[1] as f64);

        for i in 0..n {
            let alpha_i = step * (i as f64); // [0.0, 1.0)
            let x = alpha_i * pixel_width;

            for j in 0..n {
                let alpha_j = step * (j as f64); // [0.0, 1.0)
                let y = alpha_j * pixel_height;
                offsets.push([x, y]);
            }
        }

        offsets
    }

    /**
     * Returns a new image specification object with the same center and width, but
     * with resolution scaled by `subpixel_count`. Used for some antialiasing operations.
     */
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

pub fn create_buffer<T: Clone>(value: T, resolution: &[u32; 2]) -> Vec<Vec<T>> {
    vec![vec![value; resolution[1] as usize]; resolution[0] as usize]
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
    /// An optimization level of zero corresponds to the "default paramers" and one corresponds
    /// to "as fast as possible, with dramatic loss of image quality".  It is up to each fractal
    /// implementation to anchor the upper bound to a meaningful value.
    ///
    /// Note: parameters modified by this call should strictly reduce the render time, and
    /// should not change the size of the image or underlying data structures.
    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache);
}

/// Scales down an integer parameter based on a scale factor.
/// Implements clamping to ensure that scaling the value does not drop it below some
/// lower bound, but also that it does not increase the returned value.
pub fn scale_down_parameter_for_speed(lower_bound: f64, cached_value: f64, scale: f64) -> f64 {
    if cached_value < lower_bound {
        return cached_value;
    }
    let scaled_value = cached_value * scale;
    scaled_value.max(lower_bound)
}

/// Parameters shared by multiple fractal types that control how the fractal is rendered
/// to the screen.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct RenderOptions {
    /// If set to a value larger than 1, it indicates that some pixels should be skipped
    /// to allow for faster rendering. This is a particularly useful feature when trying
    /// to maintain a rapid frame-rate on larger images. It applies uniformly in both
    /// dimensions of the image. For example, setting this value to `3` will cause the
    /// image to be rendered in three-by-three blocks, with only one true "evaluation"
    /// for that block. For now, this is implemented by a zero-order hold (eg. all nine
    /// pixels are assigned the same value). Eventually we could use a better interpolation
    /// routine.
    pub downsample_stride: usize,

    /// Anti-aliasing when n > 0. Expensive, but huge improvement to image quality.
    /// The value here indicates the number of times the pixel will be sub-divided along
    /// each dimension. E.g. one subdivision along each dimension will result in four
    /// sub-pixels, and thus four rendering evaluations per pixel.
    /// 0 == no antialiasing
    /// 2 = some antialiasing (at 9x CPU time)
    /// 6 = high antialiasing (at cost of 49x CPU time)
    pub subpixel_antialiasing: u32,
}

impl SpeedOptimizer for RenderOptions {
    type ReferenceCache = RenderOptions;

    fn reference_cache(&self) -> Self::ReferenceCache {
        *self
    }

    fn set_speed_optimization_level(&mut self, level: f64, cache: &Self::ReferenceCache) {
        let max_downsample_stride = 8.0; // TODO:  param?
                                         // Note:  1.0 = no downsample stride (one sample per pixel)
        self.downsample_stride = interpolate(1.0, max_downsample_stride, level) as usize;

        self.subpixel_antialiasing =
            interpolate(cache.subpixel_antialiasing as f64, 0.0, level) as u32;
    }
}

/// The Renderable trait represents an object that can provide a point render function
/// and an image specification.
pub trait Renderable: Sync + Send + SpeedOptimizer {
    /// The type of parameters that describe the renderable object.
    type Params: Serialize + Debug;

    /// Evaluates the pixel color at a specified point in the fractal.
    fn render_point(&self, point: &[f64; 2]) -> Rgb<u8>;

    /// Access the current image specification for the renderable object.
    fn image_specification(&self) -> &ImageSpecification;

    /// Access to the rendering options:
    fn render_options(&self) -> &RenderOptions;

    /// Set the image specification for the renderable object. This may be an
    /// expensive operation, e.g. for the quadratic map objects this will trigger the
    /// color map histogram to be recomputed from scratch.
    fn set_image_specification(&mut self, image_specification: ImageSpecification);

    /// Write diagnostics information, typically to a log file, for the renderable object.
    /// This might include, e.g. parameters or a histogram summary.
    fn write_diagnostics<W: Write>(&self, writer: &mut W) -> io::Result<()>;

    /// @return a reference to the internal parametrs of the renderable object, which
    /// can then be serialized to a JSON file.
    fn params(&self) -> &Self::Params;

    /// Renders into the provided buffer.
    fn render_to_buffer(&self, buffer: &mut Vec<Vec<Rgb<u8>>>) {
        generate_scalar_image_in_place(
            self.image_specification(),
            self.render_options(),
            |point: &[f64; 2]| self.render_point(point),
            buffer,
        );
    }
}

pub fn render<T: Renderable>(
    renderable: T,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stopwatch = Stopwatch::new("Render Stopwatch".to_owned());

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(
        renderable.image_specification().resolution[0],
        renderable.image_specification().resolution[1],
    );

    serialize_to_json_or_panic(
        file_prefix.full_path_with_suffix(".json"),
        renderable.params(),
    );

    stopwatch.record_split("basic setup".to_owned());

    let image_specification = *renderable.image_specification();
    let pixel_renderer = |point: &[f64; 2]| renderable.render_point(point);
    stopwatch.record_split("build renderer".to_owned());

    let raw_data = generate_scalar_image(
        &image_specification,
        renderable.render_options(),
        pixel_renderer,
        Rgb([0, 0, 0]),
    );

    stopwatch.record_split("compute quadratic sequences".to_owned());

    // Apply color to each pixel in the image:
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = raw_data[x as usize][y as usize];
    }

    stopwatch.record_split("copy into image buffer".to_owned());
    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });
    stopwatch.record_split("write PNG".to_owned());

    let mut diagnostics_file = file_prefix.create_file_with_suffix("_diagnostics.txt");
    stopwatch.display(&mut diagnostics_file)?;
    renderable.write_diagnostics(&mut diagnostics_file)?;

    Ok(())
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

/**
 * Coordinate of a subpixel within the entire image.
 */
pub struct SubpixelIndex {
    pub pixel: [u32; 2],
    pub subpixel: [u32; 2],
}

/**
 * Used for antialiasing calculations. Splits a query into a pixel index and a
 * subpixel index.
 */
pub struct UpsampledPixelMapper {
    pixel_mapper: PixelMapper,
    subpixel_count: u32,
}

impl UpsampledPixelMapper {
    pub fn new(
        image_specification: &ImageSpecification,
        subpixel_count: u32,
    ) -> UpsampledPixelMapper {
        UpsampledPixelMapper {
            pixel_mapper: PixelMapper::new(&image_specification.upsample(subpixel_count)),
            subpixel_count,
        }
    }

    pub fn inverse_map(&self, point: &[f64; 2]) -> SubpixelIndex {
        let [x_raw, y_raw] = self.pixel_mapper.inverse_map(point);
        SubpixelIndex {
            pixel: [x_raw / self.subpixel_count, y_raw / self.subpixel_count],
            subpixel: [x_raw % self.subpixel_count, y_raw % self.subpixel_count],
        }
    }
}

/**
 * Used to store a bitmask for a square grid, with a maximum
 * bin count of 8 per size. The mask is stored in the bits of
 * a u64 integer as a space optimization.
 */
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubpixelGridMask {
    bitmask: u64,
}

impl SubpixelGridMask {
    pub fn new() -> SubpixelGridMask {
        SubpixelGridMask { bitmask: 0 }
    }

    pub fn insert(&mut self, count_per_side: u32, coordinate: [u32; 2]) {
        let [x, y] = coordinate;
        assert!(x < count_per_side);
        assert!(y < count_per_side);
        let index = x * count_per_side + y;
        self.bitmask |= 1 << index;
    }

    pub fn count_ones(&self) -> u32 {
        self.bitmask.count_ones()
    }
}

impl Default for SubpixelGridMask {
    fn default() -> Self {
        Self::new()
    }
}

pub trait PixelRenderLambda: Fn(&[f64; 2]) -> Rgb<u8> + Sync {}

impl<T> PixelRenderLambda for T where T: Fn(&[f64; 2]) -> Rgb<u8> + Sync {}

/**
 * Given image size parameters and a mapping into "regular" space used by the fractal,
 * iterate over each pixel, using a lambda function to compute the "value" of the fractal image
 * at each pixel location.
 *
 * This is used as the core implementation for pixel-based fractals, such as the Mandelbrot set and
 * the Driven-Damped Pendulum attractor.
 *
 * @param pixel_renderer:  maps from a point in the image (regular space, not pixels) to a scalar
 * value which can then later be plugged into a color map by the rendering pipeline.
 */
pub fn generate_scalar_image<F: PixelRenderLambda>(
    spec: &ImageSpecification,
    render_options: &RenderOptions,
    pixel_renderer: F,
    default_element: Rgb<u8>,
) -> Vec<Vec<Rgb<u8>>> {
    let mut raw_data: Vec<Vec<_>> = create_buffer(default_element, &spec.resolution);
    generate_scalar_image_in_place(spec, render_options, pixel_renderer, &mut raw_data);
    raw_data
}

/// Data structure to cache the details needed to do linear keyframe interpolation on
/// image (pixel) data. The expensive render calculation will be performed to compute
/// the value of pixels where `index % downsample_stride == 0` (ahead of time). Then
/// this function will read those points (and only those points) from the data view to
/// determine what the pixel value at intermediate points should be. The linear interpolation
/// is implemented with integer math, as it is very fast.
struct KeyframeLinearPixelInerpolation {
    downsample_stride: usize,
    num_complete_chunks: usize,
    terminal_reference_index: usize,
}

impl KeyframeLinearPixelInerpolation {
    fn new(data_length: usize, downsample_stride: usize) -> KeyframeLinearPixelInerpolation {
        // Number of complete "chunks" of data
        let num_chunks = data_length / downsample_stride;

        // Number of "leftover" elements at the end:
        let remainder = data_length % downsample_stride;

        // How many complete "interpolation blocks" can we process?
        let num_complete_chunks = if remainder == 0 {
            num_chunks - 1
        } else {
            num_chunks
        };
        let terminal_reference_index = num_complete_chunks * downsample_stride;

        KeyframeLinearPixelInerpolation {
            downsample_stride,
            num_complete_chunks,
            terminal_reference_index,
        }
    }

    /// Performs interpolation between keyframes to figure out the RGB value at the
    /// specified index. Uses a generic instead of a flat vector so that it can work
    /// for both a vector (inner image data) and across several vectors (outer image
    /// data) with a single algorithm.
    fn interpolate<'a, F>(&self, data_view: F, query_index: usize) -> Rgb<u8>
    where
        F: Fn(usize) -> &'a Rgb<u8>,
    {
        let chunk_index = query_index / self.downsample_stride;

        if chunk_index < self.num_complete_chunks {
            // We know the data at these indices
            let low_ref_idx = chunk_index * self.downsample_stride;
            let upp_ref_idx = low_ref_idx + self.downsample_stride;
            let local_idx = query_index - low_ref_idx;

            // Iterate through interior points and set them:
            Self::pixel_interpolate(
                data_view(low_ref_idx),
                data_view(upp_ref_idx),
                local_idx,
                self.downsample_stride,
            )
        } else {
            *data_view(self.terminal_reference_index)
        }
    }

    fn pixel_interpolate(low: &Rgb<u8>, upp: &Rgb<u8>, index: usize, distance: usize) -> Rgb<u8> {
        let delta = distance - index;
        Rgb([
            (((low[0] as usize) * delta + (upp[0] as usize) * index) / distance) as u8,
            (((low[1] as usize) * delta + (upp[1] as usize) * index) / distance) as u8,
            (((low[2] as usize) * delta + (upp[2] as usize) * index) / distance) as u8,
        ])
    }
}

/// Note: the generic `E` here can represent either an individual pixel or an entire
/// vector of pixels.
fn fill_skipped_entries<E: Clone>(downsample_stride: usize, data: &mut [E]) {
    for i in 0..data.len() {
        let offset = i % downsample_stride;
        if offset != 0 {
            data[i] = data[i - offset].clone();
        }
    }
}

fn render_single_row_within_image<F: PixelRenderLambda>(
    pixel_map_height: &LinearPixelMap,
    column_query_value: f64,
    downsample_stride: usize,
    pixel_renderer: &F,
    row: &mut [Rgb<u8>],
) {
    row.iter_mut()
        .enumerate()
        .step_by(downsample_stride)
        .for_each(|(y, elem)| {
            let im = pixel_map_height.map(y as u32);
            *elem = pixel_renderer(&[column_query_value, im]);
        });
    if downsample_stride > 1 {
        fill_skipped_entries(downsample_stride, row);
    }
}

fn wrap_renderer_with_antialiasing<F: PixelRenderLambda>(
    subpixel_antialiasing: u32,
    image_specification: &ImageSpecification,
    pixel_renderer: F,
) -> impl PixelRenderLambda {
    let subpixel_samples =
        Arc::new(image_specification.subpixel_offset_vector(subpixel_antialiasing));

    move |point: &[f64; 2]| {
        let mut sum: image::Rgb<u32> = image::Rgb([0, 0, 0]);

        for sample in subpixel_samples.iter() {
            let result = pixel_renderer(&[point[0] + sample[0], point[1] + sample[1]]);
            sum[0] += result[0] as u32;
            sum[1] += result[1] as u32;
            sum[2] += result[2] as u32;
        }

        // Scale back to the final totals:
        let count = subpixel_samples.len() as u32;

        image::Rgb([
            (sum[0] / count) as u8,
            (sum[1] / count) as u8,
            (sum[2] / count) as u8,
        ])
    }
}

/**
 * In-place version of the above function.
 */
pub fn generate_scalar_image_in_place<F: PixelRenderLambda>(
    spec: &ImageSpecification,
    render_options: &RenderOptions,
    pixel_renderer: F,
    raw_data: &mut Vec<Vec<Rgb<u8>>>,
) {
    assert_eq!(
        raw_data.len(),
        spec.resolution[0] as usize,
        "Outer dimension mismatch"
    );

    if render_options.subpixel_antialiasing > 0 {
        render_image_internal(
            spec,
            wrap_renderer_with_antialiasing(
                render_options.subpixel_antialiasing,
                spec,
                pixel_renderer,
            ),
            raw_data,
            render_options.downsample_stride,
        );
    } else {
        render_image_internal(
            spec,
            pixel_renderer,
            raw_data,
            render_options.downsample_stride,
        );
    };

    if render_options.downsample_stride > 1 {
        // This will perform bilinear interpolation over the entire image in two passes.
        //
        // PASS ONE:  interpolate between the different "inner data vectors". This pass is
        //            tricky to parallelize with the borrow checker and not cloning large
        //            data structures. It could be done with an `unsafe` block, but not worth it.
        //            Once this pass is complete, then every "inner data vector" will have
        //            the exact same sparsity pattern (at the start, some inner vectors are empty).
        //
        // PASS TWO:  interpolation within each inner data vector, in parallel. This step performs
        //            more computation that pass one, and it is trivial to parallelize beause each
        //            element in the inner data vector can be computed locally, without referencing
        //            the other inner vectors.

        let inner_count = raw_data[0].len();
        let outer_count = raw_data.len();

        // PASS ONE:
        for inner_index in 0..inner_count {
            if inner_index % render_options.downsample_stride == 0 {
                let interpolator = KeyframeLinearPixelInerpolation::new(
                    outer_count,
                    render_options.downsample_stride,
                );
                for outer_index in 0..outer_count {
                    if outer_index % render_options.downsample_stride != 0 {
                        raw_data[outer_index][inner_index] = {
                            interpolator.interpolate(
                                |outer_index: usize| -> &Rgb<u8> {
                                    &raw_data[outer_index][inner_index]
                                },
                                outer_index,
                            )
                        };
                    }
                }
            }
        }

        // PASS TWO:
        raw_data
            .par_iter_mut()
            .enumerate()
            .for_each(|(_, inner_data)| {
                let interpolator = KeyframeLinearPixelInerpolation::new(
                    inner_count,
                    render_options.downsample_stride,
                );
                for inner_index in 0..inner_data.len() {
                    if inner_index % render_options.downsample_stride != 0 {
                        inner_data[inner_index] = {
                            interpolator.interpolate(
                                |idx: usize| -> &Rgb<u8> { &inner_data[idx] },
                                inner_index,
                            )
                        };
                    }
                }
            });
    }
}

/// Implements the iteration over the image, rendering each pixel.
/// If `downsample_stride` is greater than one, then some pixels will be skipped.
/// These pixels will be filled in by linear interpolation in a following step.
fn render_image_internal<F: PixelRenderLambda>(
    spec: &ImageSpecification,
    pixel_renderer: F,
    raw_data: &mut Vec<Vec<Rgb<u8>>>,
    downsample_stride: usize,
) {
    let pixel_map_width =
        LinearPixelMap::new_from_center_and_width(spec.resolution[0], spec.center[0], spec.width);

    let pixel_map_height = LinearPixelMap::new_from_center_and_width(
        spec.resolution[1],
        spec.center[1],
        -spec.height(), // Image coordinates are upside down.
    );

    // Perform the expensive render operation.
    // Potentially down-sample in both dimensions based on `downsample_stride`.
    raw_data
        .par_iter_mut()
        .enumerate()
        .filter(|(i, _)| i % downsample_stride == 0)
        .for_each(|(x, row)| {
            let re = pixel_map_width.map(x as u32);
            assert_eq!(
                row.len(),
                spec.resolution[1] as usize,
                "Inner dimension mismatch"
            );
            render_single_row_within_image(
                &pixel_map_height,
                re,
                downsample_stride,
                &pixel_renderer,
                row,
            );
        });
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
    use std::{collections::BTreeSet, iter::FromIterator};

    use super::*;
    use ordered_float::OrderedFloat;

    #[test]
    fn test_interpolate() {
        assert_eq!(interpolate(-2.3, 3.0, 0.0), -2.3);
        assert_eq!(interpolate(-2.3, 3.0, 1.0), 3.0);
        assert_eq!(interpolate(-5.0, 3.0, 0.5), -1.0);
    }
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
    fn test_image_specification_subpixel_offset_vector() {
        // X:  pixel width:  8.0 / 4 --> 2.0;    offset with n = 4:   [0.0, 0.5, 1.0, 1.5]
        // Y:  pixel width... exactly the same!  (We derive the image height from the "square pixel" assumption).
        let image_specification = ImageSpecification {
            resolution: [4, 8],
            center: [2.0, 4.0],
            width: 8.0,
        };

        {
            let offset_vector = image_specification.subpixel_offset_vector(3);

            let mut x_offset_data = BTreeSet::new();
            let mut y_offset_data = BTreeSet::new();
            for point in offset_vector {
                x_offset_data.insert(OrderedFloat(point[0]));
                y_offset_data.insert(OrderedFloat(point[1]));
            }

            let offset_soln = BTreeSet::from_iter(
                [
                    OrderedFloat(0.0),
                    OrderedFloat(0.5),
                    OrderedFloat(1.0),
                    OrderedFloat(1.5),
                ]
                .iter()
                .cloned(),
            );

            assert_eq!(x_offset_data, offset_soln);
            assert_eq!(y_offset_data, offset_soln);
        }

        {
            let offset_vector = image_specification.subpixel_offset_vector(0);
            assert_eq!(offset_vector.len(), 1);
            assert_eq!(offset_vector[0], [0.0, 0.0]);
        }
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
    fn test_pixel_grid_mask_valid_3() {
        let mut grid_mask = super::SubpixelGridMask::new();

        assert_eq!(grid_mask.bitmask.count_ones(), 0);
        let n_grid = 3;
        grid_mask.insert(n_grid, [0, 0]);
        assert_eq!(grid_mask.bitmask.count_ones(), 1);

        grid_mask.insert(n_grid, [1, 1]);
        assert_eq!(grid_mask.bitmask.count_ones(), 2);
        grid_mask.insert(n_grid, [1, 1]);
        assert_eq!(grid_mask.bitmask.count_ones(), 2);
        grid_mask.insert(n_grid, [2, 1]);
        assert_eq!(grid_mask.bitmask.count_ones(), 3);
    }

    #[test]
    fn test_pixel_grid_mask_valid_8() {
        let mut grid_mask = super::SubpixelGridMask::new();

        assert_eq!(grid_mask.bitmask.count_ones(), 0);
        let n_grid = 8;

        for i in 0..n_grid {
            for j in 0..n_grid {
                grid_mask.insert(n_grid, [i, j]);
            }
        }
        assert_eq!({ grid_mask.count_ones() }, n_grid * n_grid);
    }

    #[test]
    #[should_panic]
    fn test_pixel_grid_mask_invalid_upp() {
        let mut grid_mask = super::SubpixelGridMask::new();
        grid_mask.insert(4, [5, 5]);
    }

    use approx::assert_relative_eq;

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
    fn test_linear_pixel_interpolation_stride_2() {
        let downsample_stride: usize = 2;
        let data = vec![
            Rgb([0, 0, 40]),
            Rgb([0, 0, 0]),
            Rgb([20, 0, 0]),
            Rgb([0, 0, 0]),
        ];
        {
            let interpolator = KeyframeLinearPixelInerpolation::new(data.len(), downsample_stride);

            let data_view = |index: usize| -> &Rgb<u8> { &data[index] };

            // Manually select the correct inputs to pixel interpolate and check that
            assert_eq!(
                KeyframeLinearPixelInerpolation::pixel_interpolate(
                    &data[0],
                    &data[2],
                    1,
                    downsample_stride
                ),
                Rgb([10, 0, 20])
            );

            // Now let the "full vector" machinery figure out the pixels
            assert_eq!(interpolator.interpolate(data_view, 1), Rgb([10, 0, 20]));
            assert_eq!(interpolator.interpolate(data_view, 3), Rgb([20, 0, 0]));

            // We don't expect to query at known points, but lets make sure it doesn't break
            assert_eq!(interpolator.interpolate(data_view, 0), Rgb([0, 0, 40]));
            assert_eq!(interpolator.interpolate(data_view, 2), Rgb([20, 0, 0]));
        }
        {
            // Now, let's add more data and try again:
            let mut data = data;
            data.push(Rgb([0, 60, 0]));

            let data_view = |index: usize| -> &Rgb<u8> { &data[index] };
            let interpolator = KeyframeLinearPixelInerpolation::new(data.len(), downsample_stride);

            // Check the first points again, but now, expect the index 3 to properly interpolate
            assert_eq!(interpolator.interpolate(data_view, 1), Rgb([10, 0, 20]));
            assert_eq!(interpolator.interpolate(data_view, 3), Rgb([10, 30, 0]));
            // Check the keyframes again, as well:
            assert_eq!(interpolator.interpolate(data_view, 0), Rgb([0, 0, 40]));
            assert_eq!(interpolator.interpolate(data_view, 2), Rgb([20, 0, 0]));
            assert_eq!(interpolator.interpolate(data_view, 4), Rgb([0, 60, 0]));
        }
    }

    #[test]
    fn test_linear_pixel_interpolation_stride_3() {
        let downsample_stride: usize = 3;
        let data = [
            Rgb([0, 0, 33]),
            Rgb([123, 123, 123]), // dummy data, should never be read
            Rgb([123, 123, 123]), // dummy data, should never be read
            Rgb([90, 60, 0]),
            Rgb([123, 123, 123]), // dummy data, should never be read
            Rgb([123, 123, 123]), // dummy data, should never be read
            Rgb([81, 140, 15]),
            Rgb([123, 123, 123]), // dummy data, should never be read
            Rgb([123, 123, 123]),
        ];

        let interpolator = KeyframeLinearPixelInerpolation::new(data.len(), downsample_stride);

        let data_view = |index: usize| -> &Rgb<u8> { &data[index] };

        // Check interpolated points
        assert_eq!(interpolator.interpolate(data_view, 1), Rgb([30, 20, 22]));
        assert_eq!(interpolator.interpolate(data_view, 2), Rgb([60, 40, 11]));
        assert_eq!(interpolator.interpolate(data_view, 4), Rgb([87, 86, 5]));
        assert_eq!(interpolator.interpolate(data_view, 5), Rgb([84, 113, 10]));

        // Check extrapolated points
        assert_eq!(interpolator.interpolate(data_view, 7), Rgb([81, 140, 15]));
        assert_eq!(interpolator.interpolate(data_view, 8), Rgb([81, 140, 15]));

        // Check keyframe points
        assert_eq!(interpolator.interpolate(data_view, 0), Rgb([0, 0, 33]));
        assert_eq!(interpolator.interpolate(data_view, 3), Rgb([90, 60, 0]));
        assert_eq!(interpolator.interpolate(data_view, 6), Rgb([81, 140, 15]));
    }
}
