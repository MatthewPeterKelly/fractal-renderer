#[cfg(test)]
mod tests {
    use std::fs;
    use fractal_renderer::fractals::common::FractalParams;
    use glob::glob;


    #[test]
    fn test_parse_all_json_files() {
        // Define the directory containing your JSON files
        let dir = "examples"; // Update this path

        // Create a pattern to match all .json files in the directory
        let pattern = format!("{}/**/*.json", dir);

        // Use glob to find all matching .json files
        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    // Read the file content
                    let content = fs::read_to_string(&path)
                        .unwrap_or_else(|_| panic!("Failed to read file: {:?}", path));

                    // Attempt to parse the JSON file into your struct
                    let result: Result<FractalParams, _> = serde_json::from_str(&content);

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
}
