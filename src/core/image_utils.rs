use image::Rgb;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageSpecification {
    // TODO:  consider using `(usize, usize)`` for data here. We don't need the vector.
    // https://github.com/MatthewPeterKelly/fractal-renderer/issues/47
    pub resolution: nalgebra::Vector2<u32>,
    pub center: nalgebra::Vector2<f64>,
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
    pub fn subpixel_offset_vector(&self, n: u32) -> Vec<nalgebra::Vector2<f64>> {
        assert!(n > 0);
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
                offsets.push(nalgebra::Vector2::new(x, y));
            }
        }

        offsets
    }

    /**
     * Returns a new image specification object with the same center and width, but
     * with resolution scaled by `subpixel_count`. Used for some antialiasing operations.
     */
    pub fn upsample(&self, subpixel_count: i32) -> ImageSpecification {
        assert!(subpixel_count > 0);
        ImageSpecification {
            resolution: self.resolution * (subpixel_count as u32),
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
    pub fn scale_to_total_pixel_count(&self, target_pixel_count: i32) -> ImageSpecification {
        assert!(target_pixel_count > 0);
        let old_pixel_count = self.resolution[0] * self.resolution[1];
        let scale = ((target_pixel_count as f64) / (old_pixel_count as f64)).sqrt();
        ImageSpecification {
            resolution: nalgebra::Vector2::new(
                (self.resolution[0] as f64 * scale).ceil() as u32,
                (self.resolution[1] as f64 * scale).ceil() as u32,
            ),
            center: self.center,
            width: self.width,
        }
    }
}

pub fn create_buffer<T: Clone>(value: T, resolution: &nalgebra::Vector2<u32>) -> Vec<Vec<T>> {
    vec![vec![value; resolution[1] as usize]; resolution[0] as usize]
}

/**
 * Describes a rectangular region in space.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ViewRectangle {
    pub center: nalgebra::Vector2<f64>,
    pub dimensions: nalgebra::Vector2<f64>,
}

impl ViewRectangle {
    pub fn from_vertices(vertices: &[nalgebra::Vector2<f64>]) -> ViewRectangle {
        assert!(!vertices.is_empty());

        let mut min_corner = vertices[0];
        let mut max_corner = vertices[0];

        for vertex in vertices.iter() {
            min_corner = min_corner.inf(vertex);
            max_corner = max_corner.sup(vertex);
        }

        let center = 0.5 * (min_corner + max_corner);
        let dimensions = max_corner - min_corner;

        ViewRectangle { center, dimensions }
    }
}

/// Parameters shared by multiple fractal types that control how the fractal is rendered
/// to the screen.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RenderOptions {
    /// If set to a value larger than 1, it indicates that some pixels should be skipped
    /// to allow for faster rendering. This is a particularily useful feature when trying
    /// to maintain a rapid frame-rate on larger images. It applies uniformly in both
    /// dimensions of the image. For example, setting this value to `3` will cause the
    /// image to be rendered in three-by-three blocks, with only one true "evaluation"
    /// for that block. For now, this is implemented by a zero-order hold (eg. all nine
    /// pixels are assigned the same value). Eventually we could use a better interpolation
    /// routine.
    pub downsample_stride: usize,
}

/// Allows a set of parameters to be dynamically adjusted to hit a target frame rate.
/// The `ReferenceCache` is used to store a reference sub-set of parameters.
pub trait SpeedOptimizer {
    type ReferenceCache;

    /// Constructs a reference cache representing the current state of the parameters.
    fn reference_cache(&self) -> Self::ReferenceCache;

    /// Modifies the parameters of the image in-place.
    /// An optimization level of zero corresponds to the "default paramers", with positive
    /// integers corresponding to progressively faster render times (and thus lower quality).
    ///
    /// Note: parameters modified by this call should strictly reduce the render time, and
    /// should not change the size of the image or underlying data structures.
    fn set_speed_optimization_level(&mut self, level: u32, cache: &Self::ReferenceCache);
}

/// The Renderable trait represents an object that can provide a point render function
/// and an image specification.
pub trait Renderable: Sync + Send + SpeedOptimizer {
    /// The type of parameters that describe the renderable object.
    type Params: Serialize + Debug;

    /// Evaluates the pixel color at a specified point in the fractal.
    fn render_point(&self, point: &nalgebra::Vector2<f64>) -> Rgb<u8>;

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
            |point: &nalgebra::Vector2<f64>| self.render_point(point),
            buffer,
        );
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
    pub resolution: nalgebra::Vector2<u32>,
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
    pub fn inverse_map(&self, point: f64) -> i32 {
        ((point - self.offset) / self.slope) as i32
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

    pub fn inverse_map(&self, point: &nalgebra::Vector2<f64>) -> (i32, i32) {
        (
            self.width.inverse_map(point[0]),
            self.height.inverse_map(point[1]),
        )
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
    pub pixel: (i32, i32),
    pub subpixel: (i32, i32),
}

/**
 * Used for antialiasing calculations. Splits a query into a pixel index and a
 * subpixel index.
 */
pub struct UpsampledPixelMapper {
    pixel_mapper: PixelMapper,
    subpixel_count: i32,
}

impl UpsampledPixelMapper {
    pub fn new(
        image_specification: &ImageSpecification,
        subpixel_count: i32,
    ) -> UpsampledPixelMapper {
        UpsampledPixelMapper {
            pixel_mapper: PixelMapper::new(&image_specification.upsample(subpixel_count)),
            subpixel_count,
        }
    }

    pub fn inverse_map(&self, point: &nalgebra::Vector2<f64>) -> SubpixelIndex {
        let (x_raw, y_raw) = self.pixel_mapper.inverse_map(point);
        SubpixelIndex {
            pixel: (x_raw / self.subpixel_count, y_raw / self.subpixel_count),
            subpixel: (x_raw % self.subpixel_count, y_raw % self.subpixel_count),
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

    pub fn insert(&mut self, count_per_side: i32, coordinate: (i32, i32)) {
        let (x, y) = coordinate;
        assert!(x >= 0 && x < count_per_side);
        assert!(y >= 0 && y < count_per_side);
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

pub trait PixelRenderLambda: Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + Sync {}

impl<T> PixelRenderLambda for T where T: Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + Sync {}

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








// pub trait IntegerInterpolate<T> {
//     fn integer_interpolate(low: &T, upp: &T, index: usize, distance: usize) -> T;
// }

// impl IntegerInterpolate<Vec<Rgb<u8>>> for Vec<Rgb<u8>> {
//     fn integer_interpolate(
//         low: &Vec<Rgb<u8>>,
//         upp: &Vec<Rgb<u8>>,
//         index: usize,
//         distance: usize,
//     ) -> Vec<Rgb<u8>> {
//         let mut value = low.clone();
//         for i in 0..low.len() {
//             value[i] = Rgb::<u8>::integer_interpolate(&low[i], &upp[i], index, distance);
//         }
//         value
//     }
// }

// fn bar(input_a: &u32, input_b: &u32, output: &mut u32) {
//     *output = input_a + input_b;
// }

// fn dummy(foo: &mut Vec<u32>) {
//     let (left, right) = foo.split_at_mut(1);
//     let (middle, rest) = right.split_at_mut(1);
//     bar(&left[0], &rest[0], &mut middle[0]);
// }

struct LinearPixelInerpolation {
    data_length: usize,
    downsample_stride: usize,
    num_complete_chunks: usize,
    terminal_reference_index: usize,
}

impl LinearPixelInerpolation {
    fn new(data_length: usize, downsample_stride: usize) -> LinearPixelInerpolation {
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

        LinearPixelInerpolation {
            data_length,
            downsample_stride,
            num_complete_chunks,
            terminal_reference_index,
        }
    }

    fn interpolate<'a, F>(&self, data_view: F, query_index: usize) -> Rgb<u8>
    where
        F: Fn(usize) -> &'a Rgb<u8>,
    {
        let chunk_index = query_index % self.downsample_stride;
        if (chunk_index < self.num_complete_chunks) {
            // We know the data at these indices
            let low_ref_idx = chunk_index * self.downsample_stride;
            let upp_ref_idx = low_ref_idx + self.downsample_stride;
            // Iterate through interior points and set them:
            Self::pixel_interpolate(
                data_view(low_ref_idx),
                data_view(upp_ref_idx),
                query_index - low_ref_idx,
                self.downsample_stride,
            )
        } else {
            data_view(self.terminal_reference_index).clone()
        }
    }

    fn pixel_interpolate(low: &Rgb<u8>, upp: &Rgb<u8>, index: usize, distance: usize) -> Rgb<u8> {
        let mut value = low.clone();
        for i in 0..3 {
            value[i] = (((low[i] as usize) * (distance - index) + (upp[i] as usize) * index)
                / distance) as u8;
        }
        value
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
            *elem = pixel_renderer(&nalgebra::Vector2::<f64>::new(column_query_value, im));
        });
    if downsample_stride > 1 {
        fill_skipped_entries(downsample_stride, row);
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
    let pixel_map_width =
        LinearPixelMap::new_from_center_and_width(spec.resolution[0], spec.center[0], spec.width);

    let pixel_map_height = LinearPixelMap::new_from_center_and_width(
        spec.resolution[1],
        spec.center[1],
        -spec.height(), // Image coordinates are upside down.
    );

    raw_data
        .par_iter_mut()
        .enumerate()
        .filter(|(i, _)| i % render_options.downsample_stride == 0)
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
                render_options.downsample_stride,
                &pixel_renderer,
                row,
            );
        });

    if render_options.downsample_stride > 1 {
        // First pass through the image. Interpolate between populated entries.
        // Now we go from a 2D-sparse pattern to a 1D sparse pattern.
        let inner_interpolator = LinearPixelInerpolation::new(
            spec.resolution[1] as usize,
            render_options.downsample_stride,
        );
        raw_data
            .par_iter_mut()
            .enumerate()
            .filter(|(i, _)| i % render_options.downsample_stride == 0)
            .for_each(|(x, row)| {
                let data_view = |i: usize| -> &Rgb<u8> { &row[i] };
                for index in 0..row.len() {
                    if index % render_options.downsample_stride != 0 {
                        row[index] = inner_interpolator.interpolate(&data_view, query_index);
                    }
                }
            });

        // Second pass through the data. Now we fill in the other dimension.
        // This one is harder to parallelize, as we're slicing the 2D data the other way.

        // NOTE:  we might want to flip the order and do the parallel version as the second pass,
        // as we're filling in more data that way.
        raw_data
            .par_iter_mut()
            .enumerate()
            .filter(|(i, _)| i % render_options.downsample_stride != 0)
            .for_each(|(x, row)| {
                for index in 0..row.len() {
                    let data_view = |i: usize| -> &Rgb<u8> {
                        // TODO:  correctly capture the index going the other way.
                    };
                    row[index] = inner_interpolator.interpolate(data_view, query_index);
                }
            });
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
    use std::{collections::BTreeSet, iter::FromIterator};

    use super::*;
    use nalgebra::Vector2;
    use ordered_float::OrderedFloat;

    #[test]
    fn test_view_port_from_vertices() {
        let vertices = vec![
            nalgebra::Vector2::new(1.0, 2.0),
            nalgebra::Vector2::new(3.0, 5.0),
            nalgebra::Vector2::new(-1.0, -2.0),
            nalgebra::Vector2::new(2.0, 3.0),
        ];

        let view_rectangle = ViewRectangle::from_vertices(&vertices);

        assert_eq!(view_rectangle.center, nalgebra::Vector2::new(1.0, 1.5));
        assert_eq!(view_rectangle.dimensions, nalgebra::Vector2::new(4.0, 7.0));
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn test_view_port_empty_vertices() {
        let vertices: Vec<nalgebra::Vector2<f64>> = Vec::new();
        ViewRectangle::from_vertices(&vertices);
    }

    #[test]
    fn test_image_specification_subpixel_offset_vector() {
        // X:  pixel width:  8.0 / 4 --> 2.0;    offset with n = 4:   [0.0, 0.5, 1.0, 1.5]
        // Y:  pixel width... exactly the same!  (We derive the image height from the "square pixel" assumption).
        let image_specification = ImageSpecification {
            resolution: nalgebra::Vector2::new(4, 8),
            center: nalgebra::Vector2::new(2.0, 4.0),
            width: 8.0,
        };

        {
            // n > 1
            let offset_vector = image_specification.subpixel_offset_vector(4);

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
            // n = 1
            let offset_vector = image_specification.subpixel_offset_vector(1);
            assert_eq!(offset_vector.len(), 1);
            assert_eq!(offset_vector[0], nalgebra::Vector2::new(0.0, 0.0));
        }
    }

    #[test]
    fn test_image_specification_height() {
        let image_specification = ImageSpecification {
            resolution: nalgebra::Vector2::new(5, 23),
            center: nalgebra::Vector2::new(2.6, 3.4),
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
        grid_mask.insert(n_grid, (0, 0));
        assert_eq!(grid_mask.bitmask.count_ones(), 1);

        grid_mask.insert(n_grid, (1, 1));
        assert_eq!(grid_mask.bitmask.count_ones(), 2);
        grid_mask.insert(n_grid, (1, 1));
        assert_eq!(grid_mask.bitmask.count_ones(), 2);
        grid_mask.insert(n_grid, (2, 1));
        assert_eq!(grid_mask.bitmask.count_ones(), 3);
    }

    #[test]
    fn test_pixel_grid_mask_valid_8() {
        let mut grid_mask = super::SubpixelGridMask::new();

        assert_eq!(grid_mask.bitmask.count_ones(), 0);
        let n_grid = 8;

        for i in 0..n_grid {
            for j in 0..n_grid {
                grid_mask.insert(n_grid, (i, j));
            }
        }
        assert_eq!(grid_mask.count_ones() as i32, n_grid * n_grid);
    }

    #[test]
    #[should_panic]
    fn test_pixel_grid_mask_invalid_upp() {
        let mut grid_mask = super::SubpixelGridMask::new();
        grid_mask.insert(4, (5, 5));
    }

    #[test]
    #[should_panic]
    fn test_pixel_grid_mask_invalid_low() {
        let mut grid_mask = super::SubpixelGridMask::new();
        grid_mask.insert(6, (-1, 5));
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
            resolution: Vector2::new(800, 600),
            center: Vector2::new(0.0, 0.0),
            width: 1.0,
        };

        let scaled_spec = image_spec.scale_to_total_pixel_count(32);
        assert_eq!(scaled_spec.center, image_spec.center);
        assert_eq!(scaled_spec.width, image_spec.width);
        assert_eq!(scaled_spec.resolution, Vector2::new(7, 5));
    }
}
