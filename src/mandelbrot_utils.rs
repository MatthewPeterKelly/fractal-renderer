
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
}
