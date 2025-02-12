/**
 * This module is used to generate fractals using the "chaos game" method,
 * in which a discrete sequence of points is sampled, and rendering those
 * points will converge to some fractal.
 */
use image::Pixel;

use crate::core::{
    file_io::FilePrefix,
    histogram::Histogram,
    image_utils::{ImageSpecification, SubpixelGridMask, UpsampledPixelMapper},
};

use super::{image_utils::write_image_to_file_or_panic, stopwatch::Stopwatch};

pub struct ColoredPoint {
    pub point: nalgebra::Vector2<f64>,
    pub color: image::Rgb<u8>,
}

/**
 * Renders a fractal defined by randomly generated sequence of points from a carefully crafted distribution.
 * The user sets up the distribution, and this function samples from the distribution and handles all of the
 * file generation and diagnostics.
 */
pub fn chaos_game_render<D>(
    background_color: image::Rgb<u8>,
    distribution_generator: &mut D,
    sample_count: u32,
    subpixel_antialiasing: i32,
    image_specification: &ImageSpecification,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>>
where
    D: FnMut() -> ColoredPoint,
{
    let mut stopwatch = Stopwatch::new("Chaos Game Stopwatch".to_owned());

    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::new(
        image_specification.resolution[0],
        image_specification.resolution[1],
    );

    // Create a second buffer in which to store the antialiasing mask:
    let mut subpixel_mask = nalgebra::DMatrix::from_element(
        image_specification.resolution[0] as usize,
        image_specification.resolution[1] as usize,
        SubpixelGridMask::new(),
    );

    for (_, _, pixel) in imgbuf.enumerate_pixels_mut() {
        *pixel = background_color;
    }

    let pixel_mapper = UpsampledPixelMapper::new(image_specification, subpixel_antialiasing);

    stopwatch.record_split("setup".to_owned());

    for _ in 0..sample_count {
        let colored_point = distribution_generator();
        let index = pixel_mapper.inverse_map(&colored_point.point);
        let (x, y) = index.pixel;

        if let Some(pixel) = imgbuf.get_pixel_mut_checked(x as u32, y as u32) {
            *pixel = colored_point.color;
            subpixel_mask[(x as usize, y as usize)].insert(subpixel_antialiasing, index.subpixel)
        }
    }

    stopwatch.record_split("sampling".to_owned());

    write_image_to_file_or_panic(file_prefix.full_path_with_suffix("_raw.png"), |f| {
        imgbuf.save(f)
    });
    stopwatch.record_split("write_raw_png".to_owned());

    // Scale back the colors toward the background, based on the subpixel sample data:
    let antialiasing_scale = 1.0 / ((subpixel_antialiasing * subpixel_antialiasing) as f32);

    let histogram = Histogram::new(
        (subpixel_antialiasing * subpixel_antialiasing + 1) as usize,
        1.0,
    );

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let weight_background =
            antialiasing_scale * (subpixel_mask[(x as usize, y as usize)].count_ones() as f32);
        let weight_pixel = 1.0 - weight_background;
        pixel.apply2(&background_color, |background: u8, pixel: u8| -> u8 {
            ((background as f32) * weight_background + (pixel as f32) * weight_pixel) as u8
        });
        histogram.insert(weight_background);
    }
    stopwatch.record_split("antialiasing_post_process".to_owned());

    write_image_to_file_or_panic(file_prefix.full_path_with_suffix(".png"), |f| {
        imgbuf.save(f)
    });
    stopwatch.record_split("write_raw_png".to_owned());

    let mut diagnostics_file = file_prefix.create_file_with_suffix("_diagnostics.txt");
    stopwatch.display(&mut diagnostics_file)?;
    histogram.display(&mut diagnostics_file)?;

    Ok(())
}
