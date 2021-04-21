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



/// Used to map from pixel space into some real-valued space
#[derive(Debug, Copy, Clone)]
pub struct PixelMap {
    pub x_zero: f64,
    pub y_zero: f64,
    pub x_scale: f64,
    pub y_scale: f64,
}

impl PixelMap {
    pub fn new(n_rows: u32, n_cols: u32, center_x: f64, center_y: f64, width_x: f64) -> PixelMap {
        let height_y = width_x * (n_rows as f64) / (n_cols as f64);
        PixelMap { x_zero: center_x - 0.5 * width_x,
            y_zero: center_y - 0.5 * height_y, 
            x_scale: width_x / (n_rows as f64),
            y_scale: height_y / (n_cols as f64) }
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
