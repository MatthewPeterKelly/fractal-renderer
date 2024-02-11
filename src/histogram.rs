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

    /**
     * @return: the lower edge of the specified bin (inclusive)
     */
    pub fn lower_edge(&self, bin_index: usize) -> f64 {
        self.value_to_index_scale * (bin_index as f64)
    }

    /**
     * @return: the fraction of the population within the speficied bin
     */
    pub fn bin_fraction(&self, bin_index: usize) -> f64 {
        (self.bin_count[bin_index] as f64) / (self.total_count() as f64)
    }

    /**
     * @return: the upper edge of the specified bin (exclusive)
     */
    pub fn upper_edge(&self, bin_index: usize) -> f64 {
        self.value_to_index_scale * ((bin_index + 1) as f64)
    }

    /**
     * Print the histogram stats to the writer
     */
    pub fn display<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writeln!(writer, "total count: {}", self.total_count())?;
        for i in 0..self.bin_count.len() {
            writeln!(
                writer,
                "bins[{}]:  [{:.2}, {:.2}) --> {}  ({:.2}%)",
                i,
                self.lower_edge(i),
                self.upper_edge(i),
                self.bin_count[i],
                100.0 * self.bin_fraction(i)
            )?;
        }
        Ok(())
    }
}

pub struct PercentileMap {
    pub edge_values: Vec<f64>,
    pub value_to_index_scale: f64,
    pub bin_width: f64,
}

impl PercentileMap {
    pub fn new(histogram: Histogram) -> PercentileMap {
        let mut edge_values: Vec<f64> = Vec::with_capacity(histogram.bin_count.len());
        edge_values.extend(
            histogram
                .bin_count
                .iter()
                .map(|&count| 0.1 * (count as f64)),
        );

        PercentileMap {
            edge_values,
            value_to_index_scale: histogram.value_to_index_scale,
            bin_width,
        }
        // HACK
        // TODO
    }

    /**
     * @param value: data point, same units as would be used in the histogram
     * @return: fractional position within the population of the histogram
     */
    pub fn percentile(value: f64) -> f64 {
        0.0
    }
}
