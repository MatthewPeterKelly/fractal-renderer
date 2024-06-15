use std::path::PathBuf;

use crate::cli::ParameterFilePath;
use crate::render;

pub fn extract_base_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem() // Get the base name component of the path
        .and_then(|name| name.to_str())
        .expect("Unable to extract base name")
}

pub fn build_output_path_with_date_time(
    params: &ParameterFilePath,
    project: &str,
    datetime: &str,
) -> std::path::PathBuf {
    let mut dirs = vec!["out", project, extract_base_name(&params.params_path)];
    if params.date_time_out {
        dirs.push(datetime);
    }

    let directory_path: PathBuf = dirs.iter().collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    directory_path
}

pub fn date_time_string() -> String {
    use chrono::{Datelike, Local, Timelike};
    let local_time = Local::now();
    format!(
        "{:04}{:02}{:02}_{:02}{:02}",
        local_time.year(),
        local_time.month(),
        local_time.day(),
        local_time.hour(),
        local_time.minute()
    )
}

/**
 * Store a path and prefix together, making it easily to quickly generate
 * a collection of files with the same prefix, but separate suffixes.
 */
pub struct FilePrefix {
    pub directory_path: std::path::PathBuf,
    pub file_prefix: String,
}

impl FilePrefix {
    pub fn with_suffix(&self, suffix: &str) -> std::path::PathBuf {
        self.directory_path.join(self.file_prefix.clone() + suffix)
    }

    pub fn create_file_with_suffix(&self, suffix: &str) -> std::io::BufWriter<std::fs::File> {
        let path = self.with_suffix(suffix);
        let file = std::fs::File::create(&path)
            .unwrap_or_else(|_| panic!("failed to create file: {:?}", path));
        std::io::BufWriter::new(file)
    }
}
