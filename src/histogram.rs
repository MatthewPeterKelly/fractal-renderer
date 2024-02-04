use std::io::{self, Write};

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
        if index >= self.bin_count.len() {
            *self.bin_count.last_mut().unwrap() += 1;
        } else {
            self.bin_count[index] += 1;
        }
    }

    pub fn total_count(&self) -> u32 {
        self.bin_count.iter().sum()
    }

    // Print the histogram
    pub fn display<W: Write>(&self, mut writer: W) -> io::Result<()> {
        let total = self.total_count();
        let percent_scale = 100.0 / (total as f64);
        writeln!(writer, "total count: {}", total)?;
        for i in 0..self.bin_count.len() {
            let low_bnd = self.value_to_index_scale * (i as f64);
            let upp_bnd = self.value_to_index_scale * ((i + 1) as f64);
            let count = self.bin_count[i];
            let percent = (count as f64) * percent_scale;
            writeln!(
                writer,
                "bins[{}]:  [{:.2}, {:.2}) --> {}  ({:.2}%)",
                i, low_bnd, upp_bnd, count, percent
            )?;
        }
        Ok(())
    }
}
