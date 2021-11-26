/// Store the raw buffer in memory for an image
#[derive(Debug, Copy, Clone)]
pub struct ColoredPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct PixelIndex {
    pub row: u32,
    pub col: u32,
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

    pub fn draw_pixel(&mut self, index: PixelIndex, color: ColoredPixel) {
        let i_pixel = (self.n_pixel * (index.row * self.n_cols + index.col)) as usize;
        self.data_buffer[i_pixel + 0] = color.r;
        self.data_buffer[i_pixel + 1] = color.g;
        self.data_buffer[i_pixel + 2] = color.b;
    }

    pub fn draw_vertical_line(&mut self, start: PixelIndex, length: u32, color: ColoredPixel) {
        for i in 0..length {
            self.draw_pixel(
                PixelIndex {
                    row: (start.row + i),
                    col: start.col,
                },
                color,
            )
        }
    }

    pub fn draw_horizontal_line(&mut self, start: PixelIndex, length: u32, color: ColoredPixel) {
        for i in 0..length {
            self.draw_pixel(
                PixelIndex {
                    row: start.row,
                    col: (start.col + i),
                },
                color,
            )
        }
    }

    pub fn draw_line(&mut self, point_one: PixelIndex, point_two: PixelIndex, color: ColoredPixel) {
        //Bresenham's Line Algorithm
        let a = 2 * (point_two.row as i32 - point_one.row as i32);
        let col_delta = point_two.col as i32 - point_one.col as i32;
        let b = a - 2 * col_delta;
        let mut p = a - col_delta;
        let mut y = point_one.row;
        for x in point_one.col..=point_two.col {
            self.draw_pixel(PixelIndex { row: y, col: x }, color);
            if p < 0 {
                p = p + a;
            } else {
                y = y + 1;
                p = p + b;
            }
        }
    }
}
