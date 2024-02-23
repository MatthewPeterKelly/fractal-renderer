mod cli;
mod file_io;
mod histogram;
mod mandelbrot_core;
mod mandelbrot_search;

use clap::Parser;

use crate::cli::{CommandsEnum, FractalRendererArgs};

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();
    let datetime = file_io::date_time_string();

    match &args.command {
        Some(CommandsEnum::MandelbrotRender(params)) => {
            crate::mandelbrot_core::render_mandelbrot_set(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &crate::file_io::build_output_path_with_date_time(
                    params,
                    "mandelbrot_render",
                    &datetime,
                ),
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
                &crate::file_io::build_output_path_with_date_time(
                    params,
                    "mandelbrot_search",
                    &datetime,
                ),
            )
            .unwrap();
        }

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
