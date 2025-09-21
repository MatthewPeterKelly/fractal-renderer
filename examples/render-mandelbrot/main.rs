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
    let fractal_params = |path: &str| -> FractalParams {
        serde_json::from_str(&std::fs::read_to_string(path).expect("Unable to read param file"))
            .unwrap()
    };

    let params = ParameterFilePath {
        params_path: String::from("examples/render-mandelbrot/params.json"),
        date_time_out: false,
    };

    render_fractal(
        &fractal_params(&params.params_path),
        FilePrefix {
            directory_path: build_output_path("examples"),
            file_base: String::from("mandelbrot"),
        },
    )
    .unwrap();
}
