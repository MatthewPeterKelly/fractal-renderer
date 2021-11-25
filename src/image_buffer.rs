/// Store the raw buffer in memory for an image
#[derive(Debug, Copy, Clone)]
pub struct ColoredPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Store the raw buffer in memory for an image
#[derive(Debug, Clone)]
pub struct ImageBuffer {
    pub n_pixel: u32,
    pub n_rows: u32,
    pub n_cols: u32,
    pub data_buffer: Vec<u8>,
}

impl ImageBuffer {
    pub fn new(n_rows: u32, n_cols: u32) -> ImageBuffer {
        ImageBuffer {
            n_pixel: 3,
            n_rows,
            n_cols,
            data_buffer: vec![0; (3 * n_rows * n_cols) as usize],
        }
    }

    pub fn draw_pixel(&mut self, row: u32, col: u32, color: ColoredPixel) {
        let i_pixel = (self.n_pixel * (row * self.n_cols + col)) as usize;
        self.data_buffer[i_pixel + 0] = color.r;
        self.data_buffer[i_pixel + 1] = color.g;
        self.data_buffer[i_pixel + 2] = color.b;
    }
}
