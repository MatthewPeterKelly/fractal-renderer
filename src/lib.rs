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
}

#[cfg(test)]
mod tests {

    use std::fs::File;
    use std::io::prelude::*; // write_all

    // For reading and opening files
    use std::io::BufWriter;

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
    fn write_simple_png_file() -> std::io::Result<()> {
        // Parameters
        let max_normalized_scale = 1.0;  // [0 = black, 1 = white] 
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
        let scale = 1.0 / ((n_cols -1) as f64);
        for i_col in 0..n_cols {
            let beta = scale * (i_col as f64);
            let value: f64 = beta* U8_BIN_COUNT * max_normalized_scale;
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
