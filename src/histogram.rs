struct Histogram {
    bin_count: Vec<u32>,
    value_to_index_scale: f64,
}

impl Histogram {
    // Constructor
    fn new(num_bins: usize, max_val: f64) -> Self {
        assert!(num_bins > 0, "`num_bins` must be positive!");
        assert!(max_val > 0.0, "`max_val` must be positive!");
        let value_to_index_scale = (num_bins as f64) / max_val;
        Histogram {
            bin_count: vec![0; num_bins],
            value_to_index_scale,
        }
    }

    // Insert a data point into the histogram
    fn insert(&mut self, data: f64) {
        if data < 0.0 {
            self.bin_count[0] += 1;
            return;
        }
        let index = (data * self.value_to_index_scale) as usize;
        if (index >= self.bin_count.len()) {
            *self.bin_count.last_mut().unwrap() += 1;
        } else {
            self.bin_count[index] += 1;
        }
    }

    // Print the histogram
    fn display(&self) {
        for (i, count) in self.bin_count.iter().enumerate() {
            println!("Bin {}: [count: {:.2}]", i, count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
