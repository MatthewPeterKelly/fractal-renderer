    fn compute_newton_step(x: f64, c: f64) -> f64 {
        // x = current guess
        // c = number to compute the square root of
        //
        // want to find the solution to x^2 - c = f(x) --> 0
        // f' = 2*x
        // Newton step is:  x -f(x) / f'(x)
        0.5 * (c / x + x)
    }

    pub fn compute_square_root(c: f64) -> f64 {
        let mut x = 0.5 * (c + 1.0); // initial guess
        let mut y: f64 = 0.0;
        for iter in 0..25 {
            y = compute_newton_step(x, c);
            println!("iter: {},   x:{},  y:{}", iter, x, y);
            if (x - y).abs() < 1e-12 {
                break;
            }
            x = y;
        }
        y
    }

#[cfg(test)]
mod tests {

    #[test]
    fn square_root() {
        assert_eq!(crate::numerical_methods::compute_square_root(4.0), 2.0);
        assert_eq!(crate::numerical_methods::compute_square_root(9.0), 3.0);
        assert_eq!(
            crate::numerical_methods::compute_square_root(10234.0),
            101.16323442832382
        );
    }
}
