mod cli;
mod file_io;
mod histogram;
mod mandelbrot_core;
mod mandelbrot_search;

use clap::Parser;

use crate::cli::{CommandsEnum, FractalRendererArgs};

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
                &crate::file_io::build_output_path_with_date_time("mandelbrot_render"),
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
                &crate::file_io::build_output_path_with_date_time("mandelbrot_search"),
            )
            .unwrap();
        }

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
