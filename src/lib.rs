pub mod numerical_methods {

    fn compute_newton_step(x: f64, c: f64) -> f64 {
        // x = current guess
        // c = number to compute the square root of
        //
        // want to find the solution to x^2 - c = f(x) --> 0
        // f' = 2*x
        // Newton step is:  x -f(x) / f'(x)
        0.5 * (c / x + x)
    }

    pub fn compute_square_root(c: f64) -> f64 {
        let mut x = 0.5 * (c + 1.0); // initial guess
        let mut y: f64 = 0.0;
        for iter in 0..25 {
            y = compute_newton_step(x, c);
            println!("iter: {},   x:{},  y:{}", iter, x, y);
            if (x - y).abs() < 1e-12 {
                break;
            }
            x = y;
        }
        y
    }
}

pub mod mandelbrot_set {

    #[derive(Debug, Copy, Clone)]
    pub struct Complex {
        pub real: f64,
        pub imag: f64,
    }

    impl Complex {
        pub fn mandelbrot_update(&mut self, c: &Complex) -> () {
            let temp = self.real * self.real - self.imag * self.imag + c.real;
            self.imag = 2.0 * self.real * self.imag + c.imag;
            self.real = temp;
        }
    }
    //////////////////////////////////////////////////////////////////////////////////////////

    /// A specific pixel in the image
    #[derive(Debug, Copy, Clone)]
    pub struct Pixel {
        pub row: u32,
        pub col: u32,
        pub index: u32,
    }

    impl Pixel {
        pub fn new(row: u32, col: u32, index: u32) -> Pixel {
            Pixel { row, col, index }
        }
    }

    //////////////////////////////////////////////////////////////////////////////////////////

    /// Iterates through an image along rows
    /// 0 1 2 3
    /// 4 5 ...
    #[derive(Debug, Copy, Clone)]
    pub struct PixelIter {
        n_cols: u32, // total number of cols
        total: u32,  // total number of pixels
        index: u32,  // current index within the image
    }

    impl PixelIter {
        pub fn new(n_rows: u32, n_cols: u32) -> PixelIter {
            PixelIter {
                n_cols,
                index: 0,
                total: n_rows * n_cols,
            }
        }
    }

    impl Iterator for PixelIter {
        type Item = Pixel;

        fn next(&mut self) -> Option<Self::Item> {
            let result = if self.index < self.total {
                Some(Pixel::new(
                    self.index / self.n_cols,
                    self.index % self.n_cols,
                    self.index,
                ))
            } else {
                None
            };
            self.index += 1;
            result
        }
    }

    //////////////////////////////////////////////////////////////////////////////////////////

    /// Manage a buffer of a specific bit depth
    pub struct BufferManager {
        bit_depth: png::BitDepth,
        buffer: Vec<u16>,
    }

    impl BufferManager {
        /// Note that size of the buffer is (16 bits * size) = size * two bytes
        pub fn new(bit_depth: png::BitDepth, size: usize) -> BufferManager {
            BufferManager {
                bit_depth,
                buffer: vec![0 as u16; size],
            }
        }

        /// Compute the effective size for the selected bit depth
        /// This is used when writing data at a specific sub-index
        pub fn get_size_at_bit_depth(&self) -> usize {
            match self.bit_depth {
                png::BitDepth::One => 16 * self.buffer.len(),
                png::BitDepth::Two => 8 * self.buffer.len(),
                png::BitDepth::Four => 4 * self.buffer.len(),
                png::BitDepth::Eight => 2 * self.buffer.len(),
                png::BitDepth::Sixteen => self.buffer.len(),
            }
        }

        pub fn set_buffer_to_zero(&mut self) {
            for element in self.buffer.iter_mut() {
                *element = 0;
            }
        }

        /// Ability to read a single 16-bit element from the buffer, independent of the bit depth.
        pub fn get_concrete_element(&self, index: usize) -> u16 {
            self.buffer[index]
        }

        /// Sets a single "virtual" element, which may be at a fractional index in the underlying buffer
        /// due to the relative bit lengths.
        /// value: normalized value on [0, 1]
        /// index: virtual index, on [0, get_size_at_bit_depth)
        pub fn set_virtual_element(&mut self, value: f64, index: usize) {
            match self.bit_depth {
                // TODO:  check that value is on [0,1]?
                png::BitDepth::Sixteen => self.set_16_bit_virtual_element(value, index),
                _ => panic!("not yet implemented!"),
            }
        }

        fn set_16_bit_virtual_element(&mut self, value: f64, index: usize) {
            const BIT_SCALE: f64 = 65535.0; // 2^16 - 1
            let scaled_value = value * BIT_SCALE;
            self.buffer[index] = scaled_value as u16;
        }
    }

    //////////////////////////////////////////////////////////////////////////////////////////
}

#[cfg(test)]
mod tests {

    use std::fs::File;
    use std::io::prelude::*; // write_all

    // For reading and opening files
    use std::io::BufWriter;

    #[test]
    fn pixel_iter_test() {
        for pixel in crate::mandelbrot_set::PixelIter::new(5, 10) {
            println!("pixel: {:?}", pixel);
        }
    }

    #[test]
    fn buffer_manager_size() {
        let count = 8;
        {
            let buffer = crate::mandelbrot_set::BufferManager::new(png::BitDepth::One, count);
            assert_eq!(buffer.get_size_at_bit_depth(), count * 16);
        }
        {
            let buffer = crate::mandelbrot_set::BufferManager::new(png::BitDepth::Two, count);
            assert_eq!(buffer.get_size_at_bit_depth(), count * 8);
        }
        {
            let buffer = crate::mandelbrot_set::BufferManager::new(png::BitDepth::Four, count);
            assert_eq!(buffer.get_size_at_bit_depth(), count * 4);
        }
        {
            let buffer = crate::mandelbrot_set::BufferManager::new(png::BitDepth::Eight, count);
            assert_eq!(buffer.get_size_at_bit_depth(), count * 2);
        }
        {
            let buffer = crate::mandelbrot_set::BufferManager::new(png::BitDepth::Sixteen, count);
            assert_eq!(buffer.get_size_at_bit_depth(), count * 1);
        }
    }

    #[test]
    fn buffer_manager_16_bit_io() {
        let count = 2;
        let mut buffer = crate::mandelbrot_set::BufferManager::new(png::BitDepth::Sixteen, count);
        buffer.set_virtual_element(0.254, 0);
        buffer.set_virtual_element(0.7253, 1);
        assert_eq!(buffer.get_concrete_element(0), 16645);
        assert_eq!(buffer.get_concrete_element(1), 47532);
    }

    #[test]
    fn hello_world_file_io() -> std::io::Result<()> {
        {
            // Write a file
            let mut file = File::create("foo.txt")?;
            file.write_all(b"Hello, world!")?;
        }
        {
            // Read the file:
            let mut file = File::open("foo.txt")?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            assert_eq!(contents, "Hello, world!");
        }
        Ok(())
    }

    #[test]
    fn write_png_u4_demo() -> std::io::Result<()> {
        // Parameters
        let max_normalized_scale = 1.0; // [0 = black, 1 = white]
        let buffer_size: usize = 512;
        const U4_BIN_COUNT: f64 = 16.0;
        let n_rows = 128;
        let n_blocks = buffer_size as u32;
        let n_cols = 2 * buffer_size as u32;

        // Setup for the PNG writer object
        // let mut data_buffer: [u8; buffer_size] = [0; buffer_size];
        let mut data_buffer = vec![0.0 as u8; buffer_size];
        let file = File::create("grayscale_demo_u4.png")?;
        let ref mut w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, n_cols /*width*/, n_rows /*height*/); //
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Four);
        let mut writer = encoder.write_header().unwrap();
        let mut stream_writer = writer.stream_writer_with_size(buffer_size);

        // Populate the data for a single row
        let scale = 1.0 / (n_cols as f64);
        for i_block in 0..n_blocks {
            let i0 = 2 * i_block;
            let i1 = i0 + 1;
            // First element in the block
            let beta0 = scale * (i0 as f64);
            let value0: f64 = beta0 * U4_BIN_COUNT * max_normalized_scale;
            let value0_u = value0 as u8;
            // Sectond element in the block
            let beta1 = scale * (i1 as f64);
            let value1: f64 = beta1 * U4_BIN_COUNT * max_normalized_scale;
            let value1_u = value1 as u8;
            let value1_shift = value1_u << 4;
            // Write the elements into the buffer
            // println!("i0: {}, i1: {}, value0: {}, value1: {}, value0_u: {}, value1_u: {}, value1_shift: {}", i0, i1, value0, value1,  value0_u, value1_u, value1_shift);
            let value_sum = value0_u + value1_shift;
            data_buffer[i_block as usize] = value_sum;
        }
        // Copy that data into every row
        for _ in 0..n_rows {
            stream_writer.write(&data_buffer[0..])?;
        }
        Ok(())
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
        let file = File::create("grayscale_demo.png")?;
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
    fn complex_constructor() {
        let complex = crate::mandelbrot_set::Complex {
            real: 1.0,
            imag: 2.0,
        };
        assert_eq!(complex.real, 1.0);
        assert_eq!(complex.imag, 2.0);
    }

    #[test]
    fn mandelbrot_update() {
        let c = crate::mandelbrot_set::Complex {
            real: 0.5,
            imag: 0.0,
        };
        let mut z = crate::mandelbrot_set::Complex {
            real: 0.0,
            imag: 0.0,
        };
        z.mandelbrot_update(&c);
        assert_eq!(z.real, 0.5);
        assert_eq!(z.imag, 0.0);
        z.mandelbrot_update(&c);
        assert_eq!(z.real, 0.75);
        assert_eq!(z.imag, 0.0);
    }

    #[test]
    fn square_root() {
        assert_eq!(crate::numerical_methods::compute_square_root(4.0), 2.0);
        assert_eq!(crate::numerical_methods::compute_square_root(9.0), 3.0);
        assert_eq!(
            crate::numerical_methods::compute_square_root(10234.0),
            101.16323442832382
        );
    }
}
