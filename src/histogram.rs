pub struct Histogram {
    pub bin_count: Vec<u32>,
    pub value_to_index_scale: f64,
}

impl Histogram {
    // Constructor
    pub fn new(num_bins: usize, max_val: f64) -> Self {
        assert!(num_bins > 0, "`num_bins` must be positive!");
        assert!(max_val > 0.0, "`max_val` must be positive!");
        let value_to_index_scale = (num_bins as f64) / max_val;
        Histogram {
            bin_count: vec![0; num_bins],
            value_to_index_scale,
        }
    }

    // Insert a data point into the histogram
    pub fn insert(&mut self, data: f64) {
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
    pub fn display(&self) {
        for (i, count) in self.bin_count.iter().enumerate() {
            println!("Bin {}: [count: {:.2}]", i, count);
        }
    }
}
