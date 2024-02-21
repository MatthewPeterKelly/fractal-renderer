// use chrono::{Datelike, Local, Timelike};
use std::path::PathBuf;

pub fn build_output_path_with_date_time(name_one: &str, name_two: &str) -> std::path::PathBuf {
    let directory_path: PathBuf = ["out", name_one, name_two].iter().collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    println!("Writing to: {:?}", directory_path);
    directory_path
}

// fn date_time_string() -> String {
//     let local_time = Local::now();
//     format!(
//         "{:04}{:02}{:02}_{:02}{:02}",
//         local_time.year(),
//         local_time.month(),
//         local_time.day(),
//         local_time.hour(),
//         local_time.minute()
//     )
// }
