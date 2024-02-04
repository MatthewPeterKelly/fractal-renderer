struct Bin {
    count: u32,
    max_val: f64, // non-inclusive
}

struct Histogram {
    bins: Vec<Bin>,
    value_to_index_scale: f64,
}

impl Histogram {
    // Constructor
    fn new(num_bins: usize, max_val: f64) -> Self {
        assert!(num_bins > 0, "`num_bins` must be positive!");
        assert!(max_val > 0.0, "`max_val` must be positive!");
        let value_to_index_scale = (num_bins as f64) / max_val;
        let bin_width = max_val / (num_bins as f64);

        let bins: Vec<Bin> = (0..num_bins)
            .map(|i| Bin {
                count: 0,
                max_val: ((i + 1) as f64) * bin_width,
            })
            .collect();

        Histogram {
            bins,
            value_to_index_scale,
        }
    }

    // Insert a data point into the histogram
    fn insert(&mut self, data: f64) {
        if (data < 0.0) {
            self.bins[0].count += 1;
            return;
        }
        let index = (data * self.value_to_index_scale) as usize;
        if (index >= self.bins.len()) {
            self.bins.last_mut().unwrap().count += 1;
        } else {
            self.bins[index].count += 1;
        }
    }

    // Print the histogram
    fn display(&self) {
        for (i, bin) in self.bins.iter().enumerate() {
            println!(
                "Bin {}: [count: {:.2}, max_val: {:.2}]",
                i, bin.count, bin.max_val
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram_insert() {
        let mut hist = Histogram::new(3, 10.0);

        hist.insert(2.5);
        hist.insert(6.8);
        hist.insert(4.2);
        hist.insert(8.7);
        hist.insert(1.1);
        hist.insert(5.5);

        hist.display();

        println!("FOOD");
    }
}
