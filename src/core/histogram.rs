use std::io::{self, Write};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default, Debug)]
pub struct Histogram {
    bin_counts: Vec<AtomicU32>,
    data_to_index_scale: f32,
    bin_width: f32,
}

/**
 * Fast and simple histogram for non-negative data.
 */
impl Histogram {
    /// Constructor
    pub fn new(num_bins: usize, max_val: f32) -> Self {
        assert!(num_bins > 0, "`num_bins` must be positive!");
        assert!(max_val > 0.0, "`max_val` must be positive!");
        let data_to_index_scale = (num_bins as f32) / max_val;
        Histogram {
            bin_counts: (0..num_bins).map(|_| AtomicU32::new(0)).collect(),
            data_to_index_scale,
            bin_width: 1.0 / data_to_index_scale,
        }
    }

    /// Resets the state of the histogram to be the same as it was
    /// after being initially constructed.
    pub fn reset(&self) {
        for count in self.bin_counts.iter() {
            count.store(0, Ordering::Relaxed);
        }
    }

    /// Insert a data point into the histogram
    pub fn insert(&self, data: f32) {
        if data < 0.0 {
            self.increment_bin_count(0);
            return;
        }
        let index = (data * self.data_to_index_scale) as usize;
        if index >= self.num_bins() {
            self.increment_bin_count(self.num_bins() - 1);
        } else {
            self.increment_bin_count(index);
        }
    }

    fn increment_bin_count(&self, index: usize) {
        self.bin_counts[index].fetch_add(1, Ordering::Relaxed);
    }

    /// @return: the total number of data points that have been inserted
    /// into the histogram. This is the sum of the count in all bins.
    pub fn total_count(&self) -> u32 {
        self.bin_counts
            .iter()
            .map(|bin| bin.load(Ordering::Relaxed))
            .sum()
    }

    /// @return: the lower edge of the specified bin (inclusive)
    pub fn lower_edge(&self, bin_index: usize) -> f32 {
        self.bin_width * (bin_index as f32)
    }

    /// @return: the upper edge of the specified bin (exclusive)
    pub fn upper_edge(&self, bin_index: usize) -> f32 {
        self.bin_width * ((bin_index + 1) as f32)
    }

    /// Print the histogram stats to the writer
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "Histogram:")?;
        let total = self.total_count();
        let percent_scale = if total == 0 {
            0.0
        } else {
            100.0 / (total as f32)
        };
        writeln!(writer, "  total count: {}", total)?;
        for i in 0..self.bin_counts.len() {
            let count = self.bin_count(i);
            let percent = (count as f32) * percent_scale;
            writeln!(
                writer,
                "  bins[{}]:  [{:.2}, {:.2}) --> {}  ({:.2}%)",
                i,
                self.lower_edge(i),
                self.upper_edge(i),
                count,
                percent
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    pub fn bin_count(&self, index: usize) -> u32 {
        self.bin_counts[index].load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn bin_counts_vec(&self) -> Vec<u32> {
        self.bin_counts
            .iter()
            .map(|count| count.load(Ordering::Relaxed))
            .collect()
    }

    pub fn num_bins(&self) -> usize {
        self.bin_counts.len()
    }
}

#[derive(Debug, Default)]
pub struct CumulativeDistributionFunction {
    pub offset: Vec<f32>, // n_bins
    pub scale: Vec<f32>,  // n_bins
    pub data_to_index_scale: f32,
    pub min_data: f32, // --> maps to 0.0
    pub max_data: f32, // --> maps to 1.0
}

impl CumulativeDistributionFunction {
    pub fn new(histogram: &Histogram) -> CumulativeDistributionFunction {
        let n_bins = histogram.num_bins();
        let mut cdf = CumulativeDistributionFunction {
            offset: Vec::with_capacity(n_bins),
            scale: Vec::with_capacity(n_bins),
            data_to_index_scale: histogram.data_to_index_scale,
            min_data: histogram.lower_edge(0),
            max_data: histogram.upper_edge(n_bins - 1),
        };
        cdf.reset(histogram);
        cdf
    }

    pub fn reset(&mut self, histogram: &Histogram) {
        let n_bins = histogram.num_bins();
        self.offset.resize(n_bins, 0.0f32);
        self.scale.resize(n_bins, 0.0f32);
        let mut accumulated_count = 0;

        self.data_to_index_scale = histogram.data_to_index_scale;
        self.min_data = histogram.lower_edge(0);
        self.max_data = histogram.upper_edge(n_bins - 1);

        if histogram.total_count() == 0 {
            self.offset.iter_mut().for_each(|x| *x = 0.5);
            self.scale.iter_mut().for_each(|x| *x = 0.0);
            return;
        }

        // x = data (input)
        // y = value (output, fraction within population)
        let scale_bin_count_to_fraction = 1.0 / (histogram.total_count() as f32);
        let mut y_low = 0.0;
        for i in 0..n_bins {
            accumulated_count += histogram.bin_count(i);
            let y_upp = (accumulated_count as f32) * scale_bin_count_to_fraction;
            let x_low = histogram.lower_edge(i);
            let dy_dx = (y_upp - y_low) * histogram.data_to_index_scale;
            self.offset[i] = y_low - x_low * dy_dx;
            self.scale[i] = dy_dx;
            y_low = y_upp; // for the next iteration
        }
    }

    /**
     * @param value: data point, same units as would be used in the histogram
     * @return: fractional position within the population of the histogram on [0,1]
     *
     * Note:  if the histogram is empty, then all in-domain queries return 0.5;
     */
    pub fn percentile(&self, data: f32) -> f32 {
        if data <= self.min_data {
            return 0.0;
        }
        let bin_index = (data * self.data_to_index_scale) as usize;
        if bin_index >= self.offset.len() {
            return 1.0;
        }
        // Interesting case: linearly interpolate between edges.
        // Interpolating coefficients are pre-computed in the constructor
        self.offset[bin_index] + data * self.scale[bin_index]
    }

    /**
     * Print the CDF to the writer for debug
     */
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "CDF:")?;
        let n_bins = self.offset.len();
        writeln!(
            writer,
            "  n_bins: {}, min_data: {}, max_data: {}",
            n_bins, self.min_data, self.max_data
        )?;
        let scale = 1.0 / self.data_to_index_scale;
        for i in 0..(n_bins + 1) {
            let data = (i as f32) * scale;
            writeln!(writer, "  {:.2}  -->  {:.4}", data, self.percentile(data))?;
        }
        writeln!(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io};

    use approx::assert_relative_eq;

    use super::{CumulativeDistributionFunction, Histogram};

    #[test]
    fn test_histogram_insert_positive_data() {
        let hist = Histogram::new(5, 10.0);

        hist.insert(2.5);
        hist.insert(6.8);

        assert_eq!(hist.bin_counts_vec(), vec![0, 1, 0, 1, 0]);
    }

    #[test]
    fn test_histogram_insert_negative_data() {
        let hist = Histogram::new(5, 10.0);

        hist.insert(-3.0);
        hist.insert(-1.5);

        assert_eq!(hist.bin_counts_vec(), vec![2, 0, 0, 0, 0]);
    }

    #[test]
    fn test_histogram_reset() {
        let hist = Histogram::new(3, 12.34);
        assert_eq!(hist.bin_counts_vec(), vec![0, 0, 0]);

        hist.insert(2.0);
        hist.insert(-1.5);
        hist.insert(100.0);
        hist.insert(100.0);

        assert_eq!(hist.bin_counts_vec(), vec![2, 0, 2]);
        hist.reset();
        assert_eq!(hist.bin_counts_vec(), vec![0, 0, 0]);
    }

    #[test]
    fn test_histogram_insert_data_at_max_val() {
        let hist = Histogram::new(5, 10.0);

        hist.insert(10.0);

        assert_eq!(hist.bin_counts_vec(), vec![0, 0, 0, 0, 1]);
    }

    #[test]
    fn test_histogram_insert_data_greater_than_max_val() {
        let hist = Histogram::new(5, 10.0);

        hist.insert(12.5);

        assert_eq!(hist.bin_counts_vec(), vec![0, 0, 0, 0, 1]);
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
        let hist = Histogram::new(3, 4.0);
        hist.insert(0.3);
        hist.insert(2.3);
        hist.insert(2.6);
        println!("Histogram:");
        hist.display(&mut io::stdout())
            .expect("Failed to display on screen");
        let cdf = CumulativeDistributionFunction::new(&hist);
        println!("CDF:");
        cdf.display(&mut io::stdout())
            .expect("Failed to displayCDF on screen");
    }

    #[test]
    fn test_histogram_empty_text_display() {
        let hist = Histogram::new(3, 4.0);
        println!("Histogram:");
        hist.display(&mut io::stdout())
            .expect("Failed to display histogram on screen");
        let cdf = CumulativeDistributionFunction::new(&hist);
        println!("CDF:");
        cdf.display(&mut io::stdout())
            .expect("Failed to displayCDF on screen");
    }

    #[test]
    fn test_histogram_file_display() {
        let hist = Histogram::new(3, 9.0);
        hist.insert(0.3);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(8.4);
        fs::create_dir_all("out").expect("Unable to create 'out` directory");
        let file = std::fs::File::create("out/histogram_test_file_display.txt")
            .expect("failed to create `histogram_test_file_display.txt`");
        let mut buf_writer = io::BufWriter::new(file);
        hist.display(&mut buf_writer)
            .expect("Failed to write to file");
        let cdf = CumulativeDistributionFunction::new(&hist);
        println!("CDF:");
        cdf.display(&mut buf_writer)
            .expect("Failed to displayCDF to file");
    }

    #[test]
    fn test_histogram_utilities() {
        let hist = Histogram::new(3, 6.0);
        hist.insert(0.3);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(0.2);

        assert_eq!(hist.total_count(), 4);

        let tol = 1e-6;

        use approx::assert_relative_eq;

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
        let hist = Histogram::new(3, max_value);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(4.2);
        let cdf = super::CumulativeDistributionFunction::new(&hist);

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
    fn test_cdf_empty() {
        let max_value = 5.0;
        let hist = Histogram::new(3, max_value);
        let cdf = CumulativeDistributionFunction::new(&hist);

        let tol = 1e-6;

        // No data in the histogram, so the CDF isn't really defined. But we don't
        // want it to crash when evaluated. Here the answer we picked is zero or
        // one if out of bounds, or 0.5 if in the valid domain.
        assert_eq!(cdf.percentile(-0.2), 0.0);
        assert_eq!(cdf.percentile(7.0), 1.0);
        for data in iter_num_tools::lin_space((0.0 + tol)..=(max_value - tol), 4) {
            assert_relative_eq!(cdf.percentile(data), 0.5, epsilon = tol);
        }
    }

    #[test]
    fn test_cdf_skewed() {
        let hist = Histogram::new(3, 6.0);
        hist.insert(4.7);
        hist.insert(5.2);
        hist.insert(4.2);
        hist.insert(4.2);
        let cdf = super::CumulativeDistributionFunction::new(&hist);

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
        let hist = Histogram::new(5, 25.0);
        for _ in 0..3 {
            hist.insert(3.0);
        }
        for _ in 0..9 {
            hist.insert(12.0);
        }
        for _ in 0..12 {
            hist.insert(24.0);
        }
        let cdf = super::CumulativeDistributionFunction::new(&hist);

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
