use crate::pixel_iter::Point2d;

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

#[derive(Debug, Copy, Clone)]
pub struct MandelbrotResult {
    pub value: f64, // scaled betweeen 0 and 1, possibly interpolated
    pub count: u32, // integer number of iterations, saturated
}

fn compute_mandelbrot(c: &Point2d, max_iter: u32) -> MandelbrotResult {
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
    fn complex_constructor() {
        let complex = crate::mandelbrot_utils::Complex {
            real: 1.0,
            imag: 2.0,
        };
        assert_eq!(complex.real, 1.0);
        assert_eq!(complex.imag, 2.0);
    }

    #[test]
    fn mandelbrot_update() {
        let c = crate::mandelbrot_utils::Complex {
            real: 0.5,
            imag: 0.0,
        };
        let mut z = crate::mandelbrot_utils::Complex {
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
