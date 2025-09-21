#[cfg(test)]
mod tests {
    use fractal_renderer::{cli::color_swatch::ColorSwatchParams, fractals::common::FractalParams};
    use glob::glob;
    use serde::de::DeserializeOwned;
    use std::{any::type_name, fs, path::PathBuf};

    fn parse_all_parameter_files_or_panic<T: DeserializeOwned>(directory: &str, excludes: &[&str]) {
        let pattern = format!("{}/**/*.json", directory);
        let exclude_paths: Vec<PathBuf> = excludes.iter().map(PathBuf::from).collect();

        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    // Skip anything under any excluded directory
                    if exclude_paths.iter().any(|ex| path.starts_with(ex)) {
                        continue;
                    }

                    let content = fs::read_to_string(&path)
                        .unwrap_or_else(|_| panic!("Failed to read file: {:?}", path));

                    let result: Result<T, _> = serde_json::from_str(&content);
                    if let Err(err) = result {
                        panic!(
                            "Failed to parse JSON file: {:?} as parameter type: `{}`.\n\n{err:#?}\n",
                            path,
                            type_name::<T>(),
                        );
                    }
                }
                Err(e) => panic!("Failed to read path: {:?}. Check permissions.", e),
            }
        }
    }

    #[test]
    fn test_ensure_all_example_files_can_be_parsed() {
        let examples_lib_dir = "examples/common";
        let color_swatch_dir = "examples/visualize-color-swatch-rainbow";

        // Check most of the example code here, skilling the library and color swatch dirs.
        parse_all_parameter_files_or_panic::<FractalParams>(
            "examples",
            &[examples_lib_dir, color_swatch_dir],
        );

        // Color swatch params use an incompatible parameter type.
        parse_all_parameter_files_or_panic::<ColorSwatchParams>(color_swatch_dir, &[]);

        // Let's also check the tests directory:
        parse_all_parameter_files_or_panic::<FractalParams>(
            "tests/param_files",
            &["tests/param_files/color_swatch"],
        );
    }
}
