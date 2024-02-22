mod cli;
mod file_io;
mod histogram;
mod mandelbrot_core;
mod mandelbrot_search;

use clap::Parser;

use crate::cli::{CommandsEnum, FractalRendererArgs};

fn extract_base_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem() // Get the base name component of the path
        .and_then(|name| name.to_str())
        .expect("Unable to extract base name")
}

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();

    match &args.command {
        Some(CommandsEnum::MandelbrotRender(params)) => {
            crate::mandelbrot_core::render_mandelbrot_set(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &crate::file_io::build_output_path_with_date_time(vec![
                    "out",
                    "mandelbrot_render",
                    extract_base_name(&params.params_path),
                ]),
                "render",
            )
            .unwrap();
        }

        Some(CommandsEnum::MandelbrotSearch(params)) => {
            crate::mandelbrot_search::mandelbrot_search_render(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &crate::file_io::build_output_path_with_date_time(vec![
                    "out",
                    "mandelbrot_search",
                    extract_base_name(&params.params_path),
                ]),
            )
            .unwrap();
        }

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
