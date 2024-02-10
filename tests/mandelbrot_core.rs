#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use fractal_renderer::mandelbrot_core::LinearPixelMap;

    #[test]
    fn test_linear_pixel_map_domain_bounds() {
        let n = 7;
        let x0 = 1.23;
        let x1 = 56.2;

        let pixel_map = LinearPixelMap::new(n, x0, x1);

        let tol = 1e-6;
        assert_relative_eq!(pixel_map.map(0), x0, epsilon = tol);
        assert_relative_eq!(pixel_map.map(n - 1), x1, epsilon = tol);
    }
}
