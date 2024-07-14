use std::path::PathBuf;

pub fn extract_base_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem() // Get the base name component of the path
        .and_then(|name| name.to_str())
        .expect("Unable to extract base name")
}

pub fn build_output_path_with_date_time(
    params_path: &str,
    project: &str,
    datetime: &Option<String>,
) -> std::path::PathBuf {
    let mut dirs = vec!["out", project, extract_base_name(params_path)];
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

pub fn maybe_date_time_string(enable: bool) ->Option<String> {
    if enable {
        Option::Some(date_time_string())
    } else {
        Option::None
    }
}

/**
 * Store a path and prefix together, making it easily to quickly generate
 * a collection of files with the same prefix, but separate suffixes.
 */
pub struct FilePrefix {
    pub directory_path: std::path::PathBuf,
    pub file_base: String,
}

impl FilePrefix {
    pub fn with_suffix(&self, suffix: &str) -> std::path::PathBuf {
        self.directory_path.join(self.file_base.clone() + suffix)
    }

    pub fn create_file_with_suffix(&self, suffix: &str) -> std::io::BufWriter<std::fs::File> {
        let path = self.with_suffix(suffix);
        let file = std::fs::File::create(&path)
            .unwrap_or_else(|_| panic!("failed to create file: {:?}", path));
        std::io::BufWriter::new(file)
    }
}
