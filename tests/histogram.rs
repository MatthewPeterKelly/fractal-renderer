#[cfg(test)]
mod tests {
    use std::{fs, io};

    use approx::assert_relative_eq;
    use fractal_renderer::histogram::{Histogram, PercentileMap};

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
    fn test_percentile_uniform() {
        let mut hist = Histogram::new(3, 6.0);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(4.2);
        let cdf = PercentileMap::new(hist);

        let tol = 1e-6;

        // out-of-bounds checks:
        assert_eq!(cdf.percentile(-0.2), 0.0);
        assert_eq!(cdf.percentile(7.0), 1.0);

        // linear CDF for uniform histogram:
        for data in iter_num_tools::lin_space(0.0..=6.0, 17) {
            assert_relative_eq!(cdf.percentile(data), data * 6.0, epsilon = tol);
        }

        // TODO:
    }
}
