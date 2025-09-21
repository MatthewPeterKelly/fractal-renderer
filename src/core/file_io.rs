use std::path::PathBuf;

use serde::Serialize;
use std::fmt::Debug;

use crate::cli::args::ParameterFilePath;

pub fn extract_base_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem() // Get the base name component of the path
        .and_then(|name| name.to_str())
        .expect("Unable to extract base name")
}

pub fn build_file_prefix(params: &ParameterFilePath, command_name: &str) -> FilePrefix {
    FilePrefix {
        directory_path: build_output_path_with_date_time(
            command_name,
            &maybe_date_time_string(params.date_time_out),
        ),
        file_base: extract_base_name(&params.params_path).to_owned(),
    }
}

pub fn build_output_path_with_date_time(
    project: &str,
    datetime: &Option<String>,
) -> std::path::PathBuf {
    let mut dirs = vec!["out", project];
    if let Some(inner_datetime_str) = datetime {
        dirs.push(inner_datetime_str);
    }
    let directory_path: PathBuf = dirs.iter().collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    directory_path
}

pub fn date_time_string() -> String {
    use chrono::{Datelike, Local, Timelike};
    let local_time = Local::now();
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        local_time.year(),
        local_time.month(),
        local_time.day(),
        local_time.hour(),
        local_time.minute(),
        local_time.second()
    )
}

pub fn maybe_date_time_string(enable: bool) -> Option<String> {
    if enable {
        Option::Some(date_time_string())
    } else {
        Option::None
    }
}

pub fn serialize_to_json_or_panic<T>(filename: std::path::PathBuf, data: &T)
where
    T: Serialize + Debug,
{
    let serialized_data = serde_json::to_string(data)
        .unwrap_or_else(|_| panic!("ERROR:  Unable to serialize data: {:?}", data));
    std::fs::write(&filename, serialized_data)
        .unwrap_or_else(|_| panic!("ERROR:  Unable to write file: {:?}", filename));
}

/**
 * Store a path and prefix together, making it easily to quickly generate
 * a collection of files with the same prefix, but separate suffixes.
 */
#[derive(Clone, Debug)]
pub struct FilePrefix {
    pub directory_path: std::path::PathBuf,
    pub file_base: String,
}

impl FilePrefix {
    pub fn full_path_with_suffix(&self, suffix: &str) -> std::path::PathBuf {
        self.directory_path.join(self.file_base.clone() + suffix)
    }

    pub fn create_file_with_suffix(&self, suffix: &str) -> std::io::BufWriter<std::fs::File> {
        let filename = self.full_path_with_suffix(suffix);
        let file = std::fs::File::create(&filename)
            .unwrap_or_else(|_| panic!("ERROR:  Unable to write file: {:?}", filename));
        std::io::BufWriter::new(file)
    }

    /**
     * Edits the `directory_path` in place by joining a sub-directory, and then ensures that
     * the newly created directory exists.
     */
    pub fn create_and_step_into_sub_directory(&mut self, sub_directory: &str) {
        self.directory_path = self.directory_path.join(sub_directory);
        std::fs::create_dir_all(&self.directory_path).unwrap();
    }
}
