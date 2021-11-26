/// Store the raw buffer in memory for an image
#[derive(Debug, Copy, Clone)]
pub struct ColoredPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct PixelIndex {
    pub row: i32,
    pub col: i32,
}

/// Store the raw buffer in memory for an image
#[derive(Debug, Clone)]
pub struct ImageBuffer {
    pub n_pixel: i32,
    pub n_rows: i32,
    pub n_cols: i32,
    pub data_buffer: Vec<u8>,
}

impl ImageBuffer {
    pub fn new(n_rows: i32, n_cols: i32) -> ImageBuffer {
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

    pub fn draw_vertical_line(&mut self, start: PixelIndex, length: i32, color: ColoredPixel) {
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

    pub fn draw_horizontal_line(&mut self, start: PixelIndex, length: i32, color: ColoredPixel) {
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
        let row_delta = point_two.row - point_one.row;
        if row_delta < 0 {
            return self.draw_line(point_two, point_one, color);
        }
        let a = 2 * row_delta;
        let col_delta = point_two.col - point_one.col;
        let step: i32 = if col_delta < 0 { -1 } else { 1 };
        let b = a - 2 * col_delta;
        let mut p = a - col_delta;
        assert!(row_delta.abs() < col_delta.abs());
        let mut y = point_one.row;
        for x in point_one.col..=point_two.col {
            self.draw_pixel(PixelIndex { row: y, col: x }, color);
            if p < 0 {
                p = p + a;
            } else {
                y = y + step;
                p = p + b;
            }
        }
    }
}
