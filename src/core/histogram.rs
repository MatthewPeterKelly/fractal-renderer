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
    // TODO:  template on data type
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
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "Histogram:")?;
        let total = self.total_count();
        let percent_scale = 100.0 / (total as f64);
        writeln!(writer, "  total count: {}", total)?;
        for i in 0..self.bin_count.len() {
            let count = self.bin_count[i];
            let percent = (count as f64) * percent_scale;
            writeln!(
                writer,
                "  bins[{}]:  [{:.2}, {:.2}) --> {}  ({:.2}%)",
                i,
                self.lower_edge(i),
                self.upper_edge(i),
                self.bin_count[i],
                percent
            )?;
        }
        writeln!(writer)?;
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
            let data = (i as f64) * scale;
            writeln!(writer, "  {:.1}  -->  {:.4}", data, self.percentile(data))?;
        }
        writeln!(writer)?;
        Ok(())
    }
}


mod tests {
    use std::{fs, io};

    use approx::assert_relative_eq;

    use crate::core::histogram::{CumulativeDistributionFunction, Histogram};

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
        hist.display(&mut io::stdout())
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
        let mut buf_writer = io::BufWriter::new(file);
        hist.display(&mut buf_writer)
            .expect("Failed to write to file");
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
    fn test_cdf_uniform() {
        let max_value = 6.0;
        let mut hist = Histogram::new(3, max_value);
        hist.insert(1.3);
        hist.insert(2.6);
        hist.insert(4.2);
        let cdf = CumulativeDistributionFunction::new(&hist);

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
    fn test_cdf_skewed() {
        let mut hist = Histogram::new(3, 6.0);
        hist.insert(4.7);
        hist.insert(5.2);
        hist.insert(4.2);
        hist.insert(4.2);
        let cdf = CumulativeDistributionFunction::new(&hist);

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
        let mut hist = Histogram::new(5, 25.0);
        for _ in 0..3 {
            hist.insert(3.0);
        }
        for _ in 0..9 {
            hist.insert(12.0);
        }
        for _ in 0..12 {
            hist.insert(24.0);
        }
        let cdf = CumulativeDistributionFunction::new(&hist);

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
