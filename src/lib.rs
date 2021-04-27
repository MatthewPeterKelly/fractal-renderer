mod mandelbrot_utils;
mod numerical_methods; // unused, but included so that tests are run
mod pixel_iter; // unused, but included so that tests are run

pub mod mandelbrot_set {
    // TODO:  rename this and move to a different file...

    //////////////////////////////////////////////////////////////////////////////////////////

    /// Manage a buffer of a specific bit depth
    pub struct BufferManager {
        bit_depth: png::BitDepth,
        buffer: Vec<u8>,
        size: usize,
    }

    /// A class for managing a data buffer and writing image elements. It allows the image depth
    /// to be abstracted away: the user provides a floating point value on [0,1) and that will
    /// be converted to the correct underlying inter representation.
    /// Note: using a smaller bit-depth is not always useful for reducing the file size of the
    /// image, although it does make the size of the buffer smaller. For example, in some of
    /// the test images the 8 bit image was actually smaller than the 4 bit image file.
    impl BufferManager {
        pub fn new(bit_depth: png::BitDepth, n_virtual_elements: usize) -> BufferManager {
            // TODO:  input validation on element size!
            let size = match bit_depth {
                png::BitDepth::One => n_virtual_elements / 8,
                png::BitDepth::Two => n_virtual_elements / 4,
                png::BitDepth::Four => n_virtual_elements / 2,
                png::BitDepth::Eight => n_virtual_elements,
                png::BitDepth::Sixteen => n_virtual_elements * 2,
            };
            // println!("Buffer Depth: {:?}, u8 element size: {}", bit_depth, size);
            BufferManager {
                bit_depth,
                buffer: vec![0 as u8; size],
                size,
            }
        }

        pub fn size(&self) -> usize {
            self.size
        }

        pub fn data(&self) -> &[u8] {
            &self.buffer[0..]
        }

        pub fn clear(&mut self) {
            for element in self.buffer.iter_mut() {
                *element = 0;
            }
        }

        /// Sets a single "virtual" element, which may be a different size from a single
        /// element in the underlying buffer
        /// index: virtual index, on [0, n_virtual_elements)
        /// value: normalized value on [0, 1)   
        ///    -- Note: upper edge of the set is open!  1.0 will wrap onto 0.0
        pub fn set_virtual_element(&mut self, index: usize, value: f64) {
            match self.bit_depth {
                // TODO:  check that value is on [0,1]?
                png::BitDepth::Sixteen => self.set_16_bit_virtual_element(value, index),
                png::BitDepth::Eight => self.set_8_bit_virtual_element(value, index),
                png::BitDepth::Four => self.set_4_bit_virtual_element(value, index),
                png::BitDepth::Two => self.set_2_bit_virtual_element(value, index),
                png::BitDepth::One => self.set_1_bit_virtual_element(value, index),
            }
        }

        fn set_16_bit_virtual_element(&mut self, value: f64, index: usize) {
            const BIT_SCALE: f64 = 65536.0; // 2^16
            let scaled_value = value * BIT_SCALE;
            let base_index = 2 * index;
            let int_value = scaled_value as u16;
            let big_part = (int_value >> 8) as u8;
            let tiny_part = (int_value & 0x00FF) as u8;
            self.buffer[base_index] = big_part;
            self.buffer[base_index + 1] = tiny_part;
        }

        fn set_8_bit_virtual_element(&mut self, value: f64, index: usize) {
            const BIT_SCALE: f64 = 256.0; // 2^8
            let scaled_value = value * BIT_SCALE;
            self.buffer[index] = scaled_value as u8;
        }

        /// NOTE: this method is somewhat unsafe, as it requires that the buffer has a zero value in this index
        fn set_4_bit_virtual_element(&mut self, value: f64, index: usize) {
            const BIT_SCALE: f64 = 16.0; // 2^4
            let scaled_value = value * BIT_SCALE;
            let major_index = index / 2; // integer division!
            let minor_index = index % 2;
            let int_value = scaled_value as u8;
            // TODO:  figure out how to clear only the matching bits here...
            self.buffer[major_index] += int_value << (minor_index * 4);
        }

        /// NOTE: this method is somewhat unsafe, as it requires that the buffer has a zero value in this index
        fn set_2_bit_virtual_element(&mut self, value: f64, index: usize) {
            const BIT_SCALE: f64 = 4.0; // 2^2
            let scaled_value = value * BIT_SCALE;
            let major_index = index / 4; // integer division!
            let minor_index = index % 4;
            let int_value = scaled_value as u8;
            // TODO:  figure out how to clear only the matching bits here...
            self.buffer[major_index] += int_value << (minor_index * 2);
        }

        /// NOTE: this method is somewhat unsafe, as it requires that the buffer has a zero value in this index
        fn set_1_bit_virtual_element(&mut self, value: f64, index: usize) {
            const BIT_SCALE: f64 = 2.0; // 2^1
            let scaled_value = value * BIT_SCALE;
            let major_index = index / 8; // integer division!
            let minor_index = index % 8;
            let int_value = scaled_value as u8;
            // TODO:  figure out how to clear only the matching bits here...
            self.buffer[major_index] += int_value << minor_index;
        }
    }

    use std::fs::File;
    use std::io::prelude::*; // write_all
    use std::io::BufWriter;

    pub fn make_grayscale_test_image(bit_depth: png::BitDepth) -> std::io::Result<()> {
        // Parameters
        let n_cols: usize = 1024;
        let n_rows: usize = 256;
        let mut buffer = crate::mandelbrot_set::BufferManager::new(bit_depth, n_cols);

        // Setup for the PNG writer object
        std::fs::create_dir_all("out")?; // TODO: bundle these two lines together into a single function
        let file = File::create(format!("out/grayscale_demo_{:?}Bit.png", bit_depth))?;
        let ref mut buf_writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(
            buf_writer,
            n_cols as u32, /*width*/
            n_rows as u32, /*height*/
        ); //
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(bit_depth);
        let mut writer = encoder.write_header().unwrap();
        let mut stream_writer = writer.stream_writer_with_size(buffer.size());

        // Populate the data for a single row
        let scale = 1.0 / (n_cols as f64);
        for i_col in 0..n_cols {
            let value = (i_col as f64) * scale;
            buffer.set_virtual_element(i_col as usize, value);
        }

        // Copy that data into every row
        for _ in 0..n_rows {
            stream_writer.write(buffer.data())?;
        }

        Ok(())
    }

    //////////////////////////////////////////////////////////////////////////////////////////
}

pub fn himmelblau(x: f64, y: f64) -> f64 {
    let a = x * x + y - 11.0;
    let b = x + y * y - 7.0;
    return a * a + b * b;
}

#[cfg(test)]
mod tests {

    use std::fs::File;

    use std::io::prelude::*; // write_all

    // For reading and opening files
    use std::convert::TryInto;
    use std::io::BufWriter;

    #[test]
    fn pixel_iter_write_image_test() -> std::io::Result<()> {
        // Parameters
        let n_cols: usize = 512;
        let n_rows: usize = 512;
        let bit_depth = png::BitDepth::Eight;
        let mut buffer = crate::mandelbrot_set::BufferManager::new(bit_depth, n_cols);

        // Setup for the PNG writer object
        std::fs::create_dir_all("out")?; // TODO: bundle these two lines together into a single function
        let file = File::create("out/grayscale_demo_diagonal.png")?;
        let ref mut buf_writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(
            buf_writer,
            n_cols as u32, /*width*/
            n_rows as u32, /*height*/
        ); //
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(bit_depth);
        let mut writer = encoder.write_header().unwrap();
        let mut stream_writer = writer.stream_writer_with_size(buffer.size());

        // Iterate through the image writing data
        let scale = 1.0 / ((n_cols + n_rows - 1) as f64);
        for pixel in crate::pixel_iter::PixelIter::new(
            (n_rows as usize).try_into().unwrap(),
            (n_cols as usize).try_into().unwrap(),
        ) {
            let value = scale * ((pixel.col + pixel.row) as f64);
            buffer.set_virtual_element(pixel.col as usize, value);
            if pixel.col == (n_cols - 1).try_into().unwrap() {
                stream_writer.write(buffer.data())?;
                buffer.clear();
            }
        }
        Ok(())
    }

    #[test]
    fn hello_world_file_io() -> std::io::Result<()> {
        {
            // Write a file
            std::fs::create_dir_all("out")?; // TODO: bundle these two lines together into a single function
            let mut file = File::create("out/foo.txt")?;
            file.write_all(b"Hello, world!")?;
        }
        {
            // Read the file:
            let mut file = File::open("out/foo.txt")?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            assert_eq!(contents, "Hello, world!");
        }
        Ok(())
    }

    #[test]
    fn write_png_u1_demo() -> std::io::Result<()> {
        crate::mandelbrot_set::make_grayscale_test_image(png::BitDepth::One)
    }

    #[test]
    fn write_png_u2_demo() -> std::io::Result<()> {
        crate::mandelbrot_set::make_grayscale_test_image(png::BitDepth::Two)
    }

    #[test]
    fn write_png_u4_demo() -> std::io::Result<()> {
        crate::mandelbrot_set::make_grayscale_test_image(png::BitDepth::Four)
    }

    #[test]
    fn write_png_u8_demo() -> std::io::Result<()> {
        crate::mandelbrot_set::make_grayscale_test_image(png::BitDepth::Eight)
    }

    #[test]
    fn write_png_u16_demo() -> std::io::Result<()> {
        crate::mandelbrot_set::make_grayscale_test_image(png::BitDepth::Sixteen)
    }

    #[test]
    fn write_png_gradient_demo() -> std::io::Result<()> {
        // Parameters
        let max_normalized_scale = 1.0; // [0 = black, 1 = white]
        const BUFFER_SIZE: usize = 1024;
        const U8_BIN_COUNT: f64 = 256.0;
        let n_rows = 128;
        let n_cols = BUFFER_SIZE as u32;

        // Setup for the PNG writer object
        let mut data_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        std::fs::create_dir_all("out")?; // TODO: bundle these two lines together into a single function
        let file = File::create("out/grayscale_demo.png")?;
        let ref mut w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, n_cols /*width*/, n_rows /*height*/); //
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut stream_writer = writer.stream_writer_with_size(BUFFER_SIZE);

        // Populate the data for a single row
        let scale = 1.0 / ((n_cols - 1) as f64);
        for i_col in 0..n_cols {
            let beta = scale * (i_col as f64);
            let value: f64 = beta * U8_BIN_COUNT * max_normalized_scale;
            let value = value as u8;
            data_buffer[i_col as usize] = value;
        }
        // Copy that data into every row
        for _ in 0..n_rows {
            stream_writer.write(&data_buffer[0..])?;
        }
        Ok(())
    }

    #[test]
    fn himmelblau_visualization() -> std::io::Result<()> {
        // Parameters
        const BUFFER_SIZE: usize = 1024;
        const U8_BIN_COUNT: f64 = 256.0;
        let n_rows = BUFFER_SIZE as u32;
        let n_cols = BUFFER_SIZE as u32;

        // Setup for the PNG writer object
        let mut data_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        std::fs::create_dir_all("out")?; // TODO: bundle these two lines together into a single function
        let file = File::create("out/himmelblau.png")?;
        let ref mut w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, n_cols /*width*/, n_rows /*height*/); //
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut stream_writer = writer.stream_writer_with_size(BUFFER_SIZE);

        // Mapping between pixels and real values
        let pixel_map = crate::pixel_iter::PixelMap::new(
            crate::pixel_iter::Point2d {
                x: n_cols as f64,
                y: n_rows as f64,
            },
            crate::pixel_iter::Point2d { x: 0.0, y: 0.0 },
            crate::pixel_iter::Point2d { x: 10.0, y: 10.0 },
        );

        // Max value above which we saturate the function value
        let scale_factor = U8_BIN_COUNT / 890.0;

        // Populate the data for a single row
        for i_row in 0..n_rows {
            for i_col in 0..n_cols {
                let point = pixel_map.map(i_row, i_col);
                let value = crate::himmelblau(point.x, point.y);
                data_buffer[i_col as usize] = (value * scale_factor) as u8;
            }
            // Copy that data into every row
            stream_writer.write(&data_buffer[0..])?;
        }
        Ok(())
    }

    #[test]
    fn zoomed_out_mandelbrot() -> std::io::Result<()> {
        // Parameters
        // const BUFFER_SIZE: usize = 1024;
        const BUFFER_SIZE: usize = 2048;
        const U8_BIN_COUNT: f64 = 256.0;
        let n_rows = BUFFER_SIZE as u32;
        let n_cols = BUFFER_SIZE as u32;

        // Setup for the PNG writer object
        let mut data_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        std::fs::create_dir_all("out")?; // TODO: bundle these two lines together into a single function
        let file = File::create("out/mandelbrot.png")?;
        let ref mut w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, n_cols /*width*/, n_rows /*height*/); //
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut stream_writer = writer.stream_writer_with_size(BUFFER_SIZE);

        // Mapping between pixels and real values
        let pixel_map = crate::pixel_iter::PixelMap::new(
            crate::pixel_iter::Point2d {
                x: n_cols as f64,
                y: n_rows as f64,
            },
            crate::pixel_iter::Point2d { x: -1.0, y: 0.5 },
            crate::pixel_iter::Point2d { x: 2.0, y: 2.0 },
        );

        // Max value above which we saturate the function value
        let scale_factor = U8_BIN_COUNT;
        let max_iter = 800;

        // Populate the data for a single row
        for i_row in 0..n_rows {
            for i_col in 0..n_cols {
                let point = pixel_map.map(i_row, i_col);
                let result = crate::mandelbrot_utils::compute_mandelbrot(&point, max_iter);
                data_buffer[i_col as usize] = (result.value * scale_factor) as u8;
            }
            // Copy that data into every row
            stream_writer.write(&data_buffer[0..])?;
        }
        Ok(())
    }
}
