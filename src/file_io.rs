use std::path::PathBuf;

pub fn build_output_path_with_date_time(
    mut names: Vec<&str>,
    maybe_date_time: Option<&str>,
) -> std::path::PathBuf {
    let date_time = date_time_string();
    if let Some(dt) = maybe_date_time {
        names.push(dt);
    }
    let directory_path: PathBuf = names.iter().collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    directory_path
}

fn date_time_string() -> String {
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
