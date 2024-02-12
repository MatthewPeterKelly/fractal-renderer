#[cfg(test)]
mod tests {
    use std::{fs, io};

    use approx::assert_relative_eq;
    use fractal_renderer::histogram::{CumulativeDistributionFunction, Histogram};

    #[test]
    fn test_histogram_insert_positive_data() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(2.5);
        hist.insert(6.8);

        assert_eq!(hist.bin_count, vec![0, 1, 0, 1, 0]);
    }

    #[test]
    fn test_histogram_insert_negative_data() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(-3.0);
        hist.insert(-1.5);

        assert_eq!(hist.bin_count, vec![2, 0, 0, 0, 0]);
    }

    #[test]
    fn test_histogram_insert_data_at_max_val() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(10.0);

        assert_eq!(hist.bin_count, vec![0, 0, 0, 0, 1]);
    }

    #[test]
    fn test_histogram_insert_data_greater_than_max_val() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(12.5);

        assert_eq!(hist.bin_count, vec![0, 0, 0, 0, 1]);
    }

    #[test]
    fn test_histogram_insert_with_zero_num_bins() {
        // This should panic due to the assertion in the constructor
        assert!(std::panic::catch_unwind(|| Histogram::new(0, 10.0)).is_err());
    }

    #[test]
    fn test_histogram_insert_with_zero_max_val() {
        // This should panic due to the assertion in the constructor
        assert!(std::panic::catch_unwind(|| Histogram::new(5, 0.0)).is_err());
    }

    #[test]
    fn test_histogram_text_display() {
        let mut hist = Histogram::new(3, 4.0);
        hist.insert(0.3);
        hist.insert(2.3);
        hist.insert(2.6);
        println!("Histogram:");
        hist.display(io::stdout())
            .expect("Failed to display on screen");
    }

    #[test]
    fn test_histogram_file_display() {
        let mut hist = Histogram::new(3, 9.0);
        hist.insert(0.3);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(8.4);
        fs::create_dir_all("out").expect("Unable to create 'out` directory");
        let file = std::fs::File::create("out/histogram_test_file_display.txt")
            .expect("failed to create `histogram_test_file_display.txt`");
        let buf_writer = io::BufWriter::new(file);
        hist.display(buf_writer).expect("Failed to write to file");
    }

    #[test]
    fn test_histogram_utilities() {
        let mut hist = Histogram::new(3, 6.0);
        hist.insert(0.3);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(0.2);

        assert_eq!(hist.total_count(), 4);

        let tol = 1e-6;

        assert_relative_eq!(hist.lower_edge(0), 0.0, epsilon = tol);
        assert_relative_eq!(hist.upper_edge(0), 2.0, epsilon = tol);
        assert_relative_eq!(hist.lower_edge(1), 2.0, epsilon = tol);
        assert_relative_eq!(hist.upper_edge(1), 4.0, epsilon = tol);
        assert_relative_eq!(hist.lower_edge(2), 4.0, epsilon = tol);
        assert_relative_eq!(hist.upper_edge(2), 6.0, epsilon = tol);
    }

    #[test]
    fn test_cdf_uniform() {
        let max_value = 6.0;
        let mut hist = Histogram::new(3, max_value);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(4.2);
        let cdf = CumulativeDistributionFunction::new(hist);

        let tol = 1e-6;

        // out-of-bounds checks:
        assert_eq!(cdf.percentile(-0.2), 0.0);
        assert_eq!(cdf.percentile(7.0), 1.0);

        // the CFG for a uniform histogram is linear
        for data in iter_num_tools::lin_space(0.0..=max_value, 17) {
            assert_relative_eq!(cdf.percentile(data), data / max_value, epsilon = tol);
        }
    }

    #[test]
    fn test_cdf_skewed() {
        let mut hist = Histogram::new(3, 6.0);
        hist.insert(4.7);
        hist.insert(5.2);
        hist.insert(4.2);
        hist.insert(4.2);
        let cdf = CumulativeDistributionFunction::new(hist);

        let tol = 1e-6;

        // check emtpy bins --> 0
        assert_eq!(cdf.percentile(1.0), 0.0);
        assert_eq!(cdf.percentile(3.0), 0.0);

        // edge of the first useful data point:
        assert_eq!(cdf.percentile(4.0), 0.0);

        // now its linear:
        assert_relative_eq!(cdf.percentile(4.1), 0.05, epsilon = tol);
        assert_relative_eq!(cdf.percentile(5.0), 0.5, epsilon = tol);
        assert_relative_eq!(cdf.percentile(5.9), 0.95, epsilon = tol);

        // upper bound
        assert_relative_eq!(cdf.percentile(6.0), 1.0, epsilon = tol);
    }

    #[test]
    fn test_cdf_interesting() {
        let mut hist = Histogram::new(5, 25.0);
        for _ in 0..3 {
            hist.insert(3.0);
        }
        for _ in 0..9 {
            hist.insert(12.0);
        }
        for _ in 0..12 {
            hist.insert(24.0);
        }
        let cdf = CumulativeDistributionFunction::new(hist);

        // edges
        assert_eq!(cdf.percentile(0.0), 0.0);
        assert_eq!(cdf.percentile(25.0), 1.0);

        // constant region
        assert_eq!(cdf.percentile(5.0), 0.125);
        assert_eq!(cdf.percentile(7.0), 0.125);
        assert_eq!(cdf.percentile(10.0), 0.125);

        // constant region
        assert_eq!(cdf.percentile(15.0), 0.5);
        assert_eq!(cdf.percentile(17.0), 0.5);
        assert_eq!(cdf.percentile(20.0), 0.5);
    }
}
