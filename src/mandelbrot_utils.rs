use crate::pixel_iter::Point2d;

#[derive(Debug, Copy, Clone)]
pub struct MandelbrotResult {
    pub value: f64, // scaled betweeen 0 and 1, possibly interpolated
    pub count: u32, // integer number of iterations, saturated
}


// Inefficient but very simple method for computing the mandelbrot set. 
// Used for unit testing and benchmarking
pub fn compute_mandelbrot(c: &Point2d, max_iter: u32) -> MandelbrotResult {
    let mut z = Point2d { x: 0.0, y: 0.0 };
    for i in 0..max_iter {
        let temp = z.x * z.x - z.y * z.y + c.x;
        z.y = 2.0 * z.x * z.y + c.y;
        z.x = temp;
        if z.x * z.x + z.y * z.y > 4.0 {
            return MandelbrotResult {
                value: (i as f64) / (max_iter as f64),
                count: i,
            };
        }
    }
    return MandelbrotResult {
        value: 1.0,
        count: max_iter,
    };
}

#[cfg(test)]
mod tests {

    #[test]
    fn compute_mandelbrot_test() {

        // point definitely in the mandelbrot set
        let c = crate::mandelbrot_utils::Point2d { x: -0.5, y: 0.0 };
        let max_iter = 100;
        let result = crate::mandelbrot_utils::compute_mandelbrot(&c, max_iter);
        assert_eq!(result.value, 1.0);
        assert_eq!(result.count, max_iter);

        // point defintiely not in the mandelbrot set
        let c = crate::mandelbrot_utils::Point2d { x: 1.0, y: 2.0 };
        let max_iter = 100;
        let result = crate::mandelbrot_utils::compute_mandelbrot(&c, max_iter);
        assert_eq!(result.value, 0.0);
        assert_eq!(result.count, 0);
    }
}
