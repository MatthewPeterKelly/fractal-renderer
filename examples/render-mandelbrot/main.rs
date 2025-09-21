use cli::args::ParameterFilePath;
use fractal_renderer::{
    cli::{self, render::render_fractal},
    core::file_io::{build_output_path, FilePrefix},
    fractals::common::FractalParams,
};

/// Run the default example for rendering the mandelbrot set.
/// ```sh
/// cargo run --example render-mandelbrot
/// ```
pub fn main() {
    let example_name = "render-mandelbrot";

    ////////////////

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
