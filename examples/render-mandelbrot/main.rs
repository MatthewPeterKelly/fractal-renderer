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
    let fractal_params = serde_json::from_str(
        &std::fs::read_to_string(String::from("examples/render-mandelbrot/params.json"))
            .expect("Unable to read param file"),
    )
    .unwrap();

    render_fractal(
        &fractal_params,
        FilePrefix {
            directory_path: build_output_path("examples"),
            file_base: String::from("mandelbrot"),
        },
    )
    .unwrap();
}
