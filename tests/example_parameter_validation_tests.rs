#[cfg(test)]
mod tests {
    use std::fs;
    use fractal_renderer::{cli::color_swatch::ColorSwatchParams, fractals::common::FractalParams};
    use glob::glob;
    use serde::de::DeserializeOwned;
    use std::any::type_name;


    fn test_parse_parameter_file<T: DeserializeOwned>(directory: &str)
    {
        let pattern = format!("{}/**/*.json", directory);

        // Use glob to find all matching .json files
        // For each match, ensure that we can (1) open the file and (2) parse it into the specified parameter type.
        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    let content = fs::read_to_string(&path)
                        .unwrap_or_else(|_| panic!("Failed to read file: {:?}", path));

                    let result: Result<T, _> = serde_json::from_str(&content);

                    assert!(
                        result.is_ok(),
                        "Failed to parse JSON file: {:?} as parameter type: `{}`. Error:\n\n{:?}\n\n",
                        path,
                        type_name::<T>(),
                        result.err()
                    );
                }
                Err(e) => panic!("Failed to read path: {:?}. Check permissions.", e),
            }
        }
    }


    #[test]
    fn test_ensure_all_example_files_can_be_parsed() {
        for sub_dir in ["/barnsley_fern",
            "/driven_damped_pendulum",
            "/mandelbrot",
            "/serpinsky"] {
                test_parse_parameter_file::<FractalParams>(&format!("examples/{}", sub_dir));
            }

        test_parse_parameter_file::<ColorSwatchParams>("examples/color_swatch");
    }
}
