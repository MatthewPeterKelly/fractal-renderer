#[cfg(test)]
mod tests {
    use std::fs;
    use fractal_renderer::{cli::color_swatch::ColorSwatchParams, fractals::common::FractalParams};
    use glob::glob;
    use serde::de::DeserializeOwned;


    fn test_parse_parameter_file<T>(directory: &str)
    where
        T: DeserializeOwned,
    {
        // Create a pattern to match all .json files in the directory
        let pattern = format!("{}/**/*.json", directory);

        // Use glob to find all matching .json files
        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    // Read the file content
                    let content = fs::read_to_string(&path)
                        .unwrap_or_else(|_| panic!("Failed to read file: {:?}", path));

                    // Attempt to parse the JSON file into the specified type
                    let result: Result<T, _> = serde_json::from_str(&content);

                    // Assert that the parsing is successful
                    assert!(
                        result.is_ok(),
                        "Failed to parse JSON file: {:?}, Error: {:?}",
                        path,
                        result.err()
                    );
                }
                Err(e) => panic!("Failed to read path: {:?}", e),
            }
        }
    }


    #[test]
    fn test_ensure_all_example_files_can_be_parsed() {
        test_parse_parameter_file::<FractalParams>("examples/barnsley_fern");
        test_parse_parameter_file::<FractalParams>("examples/driven_damped_pendulum");
        test_parse_parameter_file::<FractalParams>("examples/mandelbrot");
        test_parse_parameter_file::<FractalParams>("examples/serpinsky");
        test_parse_parameter_file::<ColorSwatchParams>("examples/color_swatch");
    }
}
