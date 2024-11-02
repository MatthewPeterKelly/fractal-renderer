#[cfg(test)]
mod tests {
    use fractal_renderer::{cli::color_swatch::ColorSwatchParams, fractals::common::FractalParams};
    use glob::glob;
    use serde::de::DeserializeOwned;
    use std::any::type_name;
    use std::fs;

    fn parse_all_parameter_files_or_panic<T: DeserializeOwned>(directory: &str) {
        let pattern = format!("{}/**/*.json", directory);

        // Use glob to find all matching .json files
        // For each match, ensure that we can (1) open the file and (2) parse it into the specified parameter type.
        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    let content = fs::read_to_string(&path)
                        .unwrap_or_else(|_| panic!("Failed to read file: {:?}", path));

                    let result: Result<T, _> = serde_json::from_str(&content);

                    match result {
                        Ok(_) => {} // Parsing was successful --> move on to the next one.
                        Err(err) => {
                            panic!(
                                "Failed to parse JSON file: {:?} as parameter type: `{}`.\n\n{:?}\n",
                                path,
                                type_name::<T>(),
                                err
                            );
                        }
                    }
                }
                Err(e) => panic!("Failed to read path: {:?}. Check permissions.", e),
            }
        }
    }

    #[test]
    fn test_ensure_all_example_files_can_be_parsed() {
        for sub_dir in [
            "/barnsley_fern",
            "/driven_damped_pendulum",
            "/mandelbrot",
            "/serpinsky",
            "/julia",
        ] {
            parse_all_parameter_files_or_panic::<FractalParams>(&format!("examples/{}", sub_dir));
        }

        parse_all_parameter_files_or_panic::<ColorSwatchParams>("examples/color_swatch");
    }
}
