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

/// Represents a point in 2D space
#[derive(Debug, Copy, Clone)]
pub struct Point2d {
    pub x: f64,
    pub y: f64,
}

impl Point2d {
    pub fn new(x: f64, y: f64) -> Point2d {
        Point2d { x, y }
    }
}

/// Used to map from pixel space into some real-valued space
#[derive(Debug, Copy, Clone)]
pub struct PixelMap {
    pub x_zero: f64,
    pub y_zero: f64,
    pub x_scale: f64,
    pub y_scale: f64,
}

impl PixelMap {
    pub fn new(
        n_rows: u32,
        n_cols: u32,
        center: Point2d,
        dims: Point2d,
    ) -> PixelMap {
        PixelMap {
            x_zero: center.x - 0.5 * dims.x,
            y_zero: center.y - 0.5 * dims.y,
            x_scale: dims.x / (n_cols as f64),
            y_scale: dims.y / (n_rows as f64),
        }
    }

    pub fn map(self, row: u32, col: u32) -> Point2d {
        return Point2d {
            x: self.x_zero + (col as f64) * self.x_scale,
            y: self.y_zero + (row as f64) * self.y_scale,
        };
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn pixel_iter_test() {
        for pixel in crate::pixel_iter::PixelIter::new(5, 10) {
            println!("pixel: {:?}", pixel);
        }
    }
}
