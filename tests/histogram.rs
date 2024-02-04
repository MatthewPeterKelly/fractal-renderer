#[cfg(test)]
mod tests {
    use fractal_renderer::histogram::Histogram;

    #[test]
    fn test_insert_positive_data() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(2.5);
        hist.insert(6.8);

        assert_eq!(hist.bin_count, vec![0, 1, 0, 1, 0]);
    }

    #[test]
    fn test_insert_negative_data() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(-3.0);
        hist.insert(-1.5);

        assert_eq!(hist.bin_count, vec![2, 0, 0, 0, 0]);
    }

    #[test]
    fn test_insert_data_at_max_val() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(10.0);

        assert_eq!(hist.bin_count, vec![0, 0, 0, 0, 1]);
    }

    #[test]
    fn test_insert_data_greater_than_max_val() {
        let mut hist = Histogram::new(5, 10.0);

        hist.insert(12.5);

        assert_eq!(hist.bin_count, vec![0, 0, 0, 0, 1]);
    }

    #[test]
    fn test_insert_with_zero_num_bins() {
        // This should panic due to the assertion in the constructor
        assert!(std::panic::catch_unwind(|| Histogram::new(0, 10.0)).is_err());
    }

    #[test]
    fn test_insert_with_zero_max_val() {
        // This should panic due to the assertion in the constructor
        assert!(std::panic::catch_unwind(|| Histogram::new(5, 0.0)).is_err());
    }
}
