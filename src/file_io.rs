use chrono::{Datelike, Local, Timelike};
use std::path::PathBuf;

pub fn build_output_path_with_date_time(directory_name: &str) -> std::path::PathBuf {
    let directory_path: PathBuf = ["out", directory_name, &date_time_string()]
        .iter()
        .collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    directory_path
}

fn date_time_string() -> String {
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
