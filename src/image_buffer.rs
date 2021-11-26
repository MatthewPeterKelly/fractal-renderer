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
        if index.row >= self.n_rows || index.col >= self.n_cols || index.row < 0 || index.col < 0 {
            return;
        }
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
        let x_del = point_two.col - point_one.col;
        let y_del = point_two.row - point_one.row;

        // TODO:  reduce code duplication here?
        if y_del.abs() < x_del.abs() {
            if x_del < 0 {
                return self.draw_line(point_two, point_one, color);
            }
            assert!(x_del >= 0);

            let a = 2 * y_del.abs();
            let step: i32 = if y_del < 0 { -1 } else { 1 };
            let b = a - 2 * x_del;
            let mut p = a - x_del;
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
        } else {
            if y_del < 0 {
                return self.draw_line(point_two, point_one, color);
            }
            assert!(y_del >= 0);

            let a = 2 * x_del.abs();
            let step: i32 = if x_del < 0 { -1 } else { 1 };
            let b = a - 2 * y_del;
            let mut p = a - y_del;
            let mut x = point_one.col;

            for y in point_one.row..=point_two.row {
                self.draw_pixel(PixelIndex { row: y, col: x }, color);
                if p < 0 {
                    p = p + a;
                } else {
                    x = x + step;
                    p = p + b;
                }
            }
        }
    }

    pub fn draw_regular_polygon(
        &mut self,
        center: PixelIndex,
        radius: f64,
        n_sides: i32,
        color: ColoredPixel,
    ) {
        let mut prev_pixel = PixelIndex {
            row: center.row,
            col: center.col + (radius as i32),
        };
        let scale = 2.0 * std::f64::consts::PI / (n_sides as f64);
        for index in 1..=n_sides {
            let angle = (index as f64) * scale;
            let x_del = radius * angle.cos();
            let y_del = radius * angle.sin();
            let pixel = PixelIndex {
                row: center.row + (y_del as i32),
                col: center.col + (x_del as i32),
            };
            self.draw_line(prev_pixel, pixel, color);
            prev_pixel = pixel;
        }
    }
}
