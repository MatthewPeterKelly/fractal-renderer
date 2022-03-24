// Note: fractal image stuff is all under numericalmethods...
use numerical_methods; // TODO:  this is a super confusing name for this crate

use std::fs::File;
use std::io::prelude::*;
// write_all
use std::io::BufWriter;

// TODO:  implement with `ImageBuffer`
fn main() {
    use numerical_methods::ddp_utils::FractalRawData;
    use numerical_methods::image_buffer::ColoredPixel;
    use numerical_methods::image_buffer::ImageBuffer;
    use numerical_methods::image_buffer::PixelIndex;
    use std::collections::HashSet;
    let mut intensity_set = HashSet::new();

    let fractal_raw_data_filename = "out/ddp_raw_data__2022123_2118";

    let generated_image_filename = fractal_raw_data_filename.to_owned() + "_image_5.png";

    /*
     * Note: set this to 1 to disable aliasing. This is the number of pixels in each
     * dimension that are collected into a single pixel in the image. For example, a
     * value of 2 --> a 2x2 block of pixels in the raw data is used to compute a single
     * pixel in the output image.
     */
    let subsample_alias: u32 = 5;

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(fractal_raw_data_filename)
        .unwrap();
    let mut deserialized_buffer = Vec::<u8>::new();
    file.read_to_end(&mut deserialized_buffer).unwrap();
    let fractal_raw_data: FractalRawData = bincode::deserialize(&deserialized_buffer[..]).unwrap();

    // Parameters
    let n_rows = fractal_raw_data.rate_count / subsample_alias;
    let n_cols = fractal_raw_data.angle_count / subsample_alias;

    // Set up the image buffer
    let mut data_buffer = ImageBuffer::new(n_rows as i32, n_cols as i32);
    let file = File::create(generated_image_filename).unwrap();
    let ref mut w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, n_cols /*width*/, n_rows /*height*/); //
    encoder.set_color(png::ColorType::RGB);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let mut stream_writer = writer.stream_writer();

    // Populate the data for a single row
    let scale = 255 / (subsample_alias * subsample_alias);
    for i_row in 0..n_rows {
        for i_col in 0..n_cols {
            let mut count = 0;
            for row_sub in 0..subsample_alias {
                for col_sub in 0..subsample_alias {
                    let basin = fractal_raw_data.data[(
                        (subsample_alias * i_col + col_sub) as usize,
                        (subsample_alias * i_row + row_sub) as usize,
                    )];
                    if basin == 0 {
                        count = count + 1;
                    }
                }
            }
            let intensity: u8 = (scale * count) as u8;
            intensity_set.insert(intensity); // debug
            let color = ColoredPixel {
                r: intensity,
                g: intensity,
                b: intensity,
            };
            // TODO: transpose here is confusing...
            let pixel_index = PixelIndex {
                row: i_col as i32,
                col: i_row as i32,
            };

            data_buffer.draw_pixel(pixel_index, color);
        }
    }

    // Dump to file
    stream_writer
        .write_all(&data_buffer.data_buffer[0..])
        .unwrap();

    // Debug:
    println!("IntensitySet: {:?}", intensity_set);
}
