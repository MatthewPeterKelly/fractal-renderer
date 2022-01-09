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

    let fractal_raw_data_filename = "out/ddp_raw_data_high_res";

    let generated_image_filename = fractal_raw_data_filename.to_owned() + "_image.png";

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
    let n_rows = fractal_raw_data.rate_count;
    let n_cols = fractal_raw_data.angle_count;

    // Set up the image buffer
    let mut data_buffer = ImageBuffer::new(n_rows as i32, n_cols as i32);
    let file = File::create(generated_image_filename).unwrap();
    let ref mut w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, n_cols /*width*/, n_rows /*height*/); //
    encoder.set_color(png::ColorType::RGB);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let mut stream_writer = writer.stream_writer();

    // Pretty colors!
    let purple = ColoredPixel {
        r: 240,
        g: 240,
        b: 255,
    };
    let black = ColoredPixel {
        r: 10,
        g: 10,
        b: 20,
    };

    // Populate the data for a single row
    for i_row in 0..n_rows {
        for i_col in 0..n_cols {
            // TODO: transpose here is confusing...
            let pixel_index = PixelIndex {
                row: i_col as i32,
                col: i_row as i32,
            };
            let basin = fractal_raw_data.data[(i_col as usize, i_row as usize)];
            if basin == 0 {
                data_buffer.draw_pixel(pixel_index, purple);
            } else {
                data_buffer.draw_pixel(pixel_index, black);
            }
        }
    }

    // Dump to file
    stream_writer
        .write_all(&data_buffer.data_buffer[0..])
        .unwrap();
}
