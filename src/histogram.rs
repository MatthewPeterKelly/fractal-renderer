struct Histogram {
    bins: Vec<u32>,
    bin_edges: (f64, f64),
    num_bins: usize,
    scale_factor: f64,
}

impl Histogram {
    // Constructor
    fn new(num_bins: usize, domain_min: f64, domain_max: f64) -> Self {
        assert!(num_bins > 0, "Number of bins must be greater than 0");
        assert!(
            domain_min < domain_max,
            "Invalid domain: min must be less than max"
        );

        let bin_width = (domain_max - domain_min) / num_bins as f64;

        let bin_edges = (domain_min, domain_max);
        let bins = vec![0; num_bins];
        let scale_factor = num_bins as f64 / (domain_max - domain_min);

        Histogram {
            bins,
            bin_edges,
            num_bins,
            scale_factor,
        }
    }

    // Insert a data point into the histogram
    fn insert(&mut self, data_point: f64) {
        let (min, _) = self.bin_edges;
        let scaled_data = (data_point - min) * self.scale_factor;

        // Perform bounds check on the integer index
        if scaled_data >= 0.0 && scaled_data < self.num_bins as f64 {
            let bin_index = scaled_data as usize;

            // Increment the corresponding bin
            self.bins[bin_index] += 1;
        }
    }

    // Print the histogram
    fn display(&self) {
        for (i, &count) in self.bins.iter().enumerate() {
            let bin_start = self.bin_edges.0
                + i as f64 * (self.bin_edges.1 - self.bin_edges.0) / self.num_bins as f64;
            let bin_end = bin_start + (self.bin_edges.1 - self.bin_edges.0) / self.num_bins as f64;

            println!("Bin {}: [{:.2}, {:.2}): {}", i, bin_start, bin_end, count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram_insert() {
        let mut hist = Histogram::new(3, 0.0, 10.0);

        hist.insert(2.5);
        hist.insert(6.8);
        hist.insert(4.2);
        hist.insert(8.7);
        hist.insert(1.1);
        hist.insert(5.5);

        assert_eq!(hist.bins, vec![2, 2, 2]);

        hist.display();

        println!("FOOD");
    }
}
