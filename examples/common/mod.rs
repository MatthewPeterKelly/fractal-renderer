use std::path::PathBuf;

use fractal_renderer::{
    cli::{color_swatch::generate_color_swatch, render::render_fractal},
    core::file_io::FilePrefix,
};

pub fn build_output_path(project: &str) -> std::path::PathBuf {
    let directory_path: PathBuf = ["out", project].iter().collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    directory_path
}

pub fn render_example_from_string(example_name: &str) {
    let params_name = String::from("examples/") + example_name + &String::from("/params.json");

    let fractal_params = serde_json::from_str(
        &std::fs::read_to_string(params_name).expect("Unable to read param file"),
    )
    .unwrap();

    render_fractal(
        &fractal_params,
        FilePrefix {
            directory_path: build_output_path(example_name),
            file_base: String::from("result"),
        },
    )
    .unwrap();
}

pub fn color_swatch_example_from_string(example_name: &str) {
    let params_name = String::from("examples/") + example_name + &String::from("/params.json");

    generate_color_swatch(
        &params_name,
        FilePrefix {
            directory_path: build_output_path(example_name),
            file_base: String::from("result"),
        },
    );
}
