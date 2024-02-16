use std::io::{self, Write};

pub struct Histogram {
    pub bin_count: Vec<u32>,
    pub data_to_index_scale: f64,
    pub bin_width: f64,
}

/**
 * Fast and simple histogram for non-negative data.
 */
impl Histogram {
    // Constructor
    pub fn new(num_bins: usize, max_val: f64) -> Self {
        assert!(num_bins > 0, "`num_bins` must be positive!");
        assert!(max_val > 0.0, "`max_val` must be positive!");
        let data_to_index_scale = (num_bins as f64) / max_val;
        Histogram {
            bin_count: vec![0; num_bins],
            data_to_index_scale,
            bin_width: 1.0 / data_to_index_scale,
        }
    }

    // Insert a data point into the histogram
    pub fn insert(&mut self, data: f64) {
        if data < 0.0 {
            self.bin_count[0] += 1;
            return;
        }
        let index = (data * self.data_to_index_scale) as usize;
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
        self.bin_width * (bin_index as f64)
    }

    /**
     * @return: the upper edge of the specified bin (exclusive)
     */
    pub fn upper_edge(&self, bin_index: usize) -> f64 {
        self.bin_width * ((bin_index + 1) as f64)
    }

    /**
     * Print the histogram stats to the writer
     */
    pub fn display<W: Write>(&self, mut writer: W) -> io::Result<()> {
        let total = self.total_count();
        let percent_scale = 100.0 / (total as f64);
        writeln!(writer, "total count: {}", total)?;
        for i in 0..self.bin_count.len() {
            let count = self.bin_count[i];
            let percent = (count as f64) * percent_scale;
            writeln!(
                writer,
                "bins[{}]:  [{:.2}, {:.2}) --> {}  ({:.2}%)",
                i,
                self.lower_edge(i),
                self.upper_edge(i),
                self.bin_count[i],
                percent
            )?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct CumulativeDistributionFunction {
    pub offset: Vec<f64>, // n_bins
    pub scale: Vec<f64>,  // n_bins
    pub data_to_index_scale: f64,
    pub min_data: f64, // --> maps to 0.0
    pub max_data: f64, // --> maps to 1.0
}

impl CumulativeDistributionFunction {
    pub fn new(histogram: &Histogram) -> CumulativeDistributionFunction {
        let scale_bin_count_to_fraction = 1.0 / (histogram.total_count() as f64);

        let n_bins = histogram.bin_count.len();
        let mut offset: Vec<f64> = Vec::with_capacity(n_bins);
        let mut scale: Vec<f64> = Vec::with_capacity(n_bins);
        let mut accumulated_count = 0;

        // x = data (input)
        // y = value (output, fraction within population)
        let mut y_low = 0.0;
        for i in 0..histogram.bin_count.len() {
            accumulated_count += histogram.bin_count[i];
            let y_upp = (accumulated_count as f64) * scale_bin_count_to_fraction;
            let x_low = histogram.lower_edge(i);
            let dy_dx = (y_upp - y_low) * histogram.data_to_index_scale;

            offset.push(y_low - x_low * dy_dx);
            scale.push(dy_dx);

            y_low = y_upp; // for the next iteration
        }

        CumulativeDistributionFunction {
            offset,
            scale,
            data_to_index_scale: histogram.data_to_index_scale,
            min_data: histogram.lower_edge(0),
            max_data: histogram.upper_edge(n_bins - 1),
        }
    }

    /**
     * @param value: data point, same units as would be used in the histogram
     * @return: fractional position within the population of the histogram
     */
    pub fn percentile(&self, data: f64) -> f64 {
        if data <= self.min_data {
            return 0.0;
        }
        if data >= self.max_data {
            return 1.0;
        }
        // Interesting case: linearly interpolate between edges.
        // Interpolating coefficients are pre-computed in the constructor
        let index = (data * self.data_to_index_scale) as usize;
        self.offset[index] + data * self.scale[index]
    }
}
