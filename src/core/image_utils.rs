use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    width: LinearPixelMap,
    height: LinearPixelMap,
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

// Use the PixelMapper to map from "point" to "pixel" space, and then
// use existing utilitites in the ImageBuffer to draw the pixel at a specific color

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
pub fn generate_scalar_image<F>(
    spec: &ImageSpecification,
    pixel_renderer: F,
) -> Vec<Vec<Option<f32>>>
where
    F: Fn(&nalgebra::Vector2<f64>) -> Option<f32> + std::marker::Sync,
{
    let mut raw_data: Vec<Vec<_>> = create_buffer(None, &spec.resolution);
    generate_scalar_image_in_place(spec, pixel_renderer, &mut raw_data);
    raw_data
}

/**
 * In-place version of the above function.
 */
pub fn generate_scalar_image_in_place<F>(
    spec: &ImageSpecification,
    pixel_renderer: F,
    raw_data: &mut Vec<Vec<Option<f32>>>,
) where
    F: Fn(&nalgebra::Vector2<f64>) -> Option<f32> + std::marker::Sync,
{
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
    raw_data.par_iter_mut().enumerate().for_each(|(x, row)| {
        let re = pixel_map_width.map(x as u32);
        assert_eq!(
            row.len(),
            spec.resolution[1] as usize,
            "Inner dimension mismatch"
        );
        row.iter_mut().enumerate().for_each(|(y, elem)| {
            let im = pixel_map_height.map(y as u32);
            *elem = pixel_renderer(&nalgebra::Vector2::<f64>::new(re, im));
        });
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
}
