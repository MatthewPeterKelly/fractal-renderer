mod cli;
mod file_io;
mod mandelbrot_core;
mod mandelbrot_search;

use clap::Parser;

use crate::cli::{CommandsEnum, FractalRendererArgs};

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();

    match &args.command {
        Some(CommandsEnum::Mandelbrot(params)) => {
            let params_json =
                std::fs::read_to_string(&params.params_path).expect("Unable to read param file");
            let params: crate::mandelbrot_core::MandelbrotParams =
                serde_json::from_str(&params_json).unwrap();
            crate::mandelbrot_core::render_mandelbrot_set(
                &params,
                &crate::file_io::build_output_path("mandelbrot"),
                "render",
            )
            .unwrap();
        }

        Some(CommandsEnum::MandelbrotSearch(params)) => {
            // Load parameters for each individual render:
            println!("Params file: {}", params.params_path);
            let render_params_str =
                std::fs::read_to_string(&params.params_path).expect("Unable to read param file");
            let params: crate::mandelbrot_search::MandelbrotSearchParams =
                serde_json::from_str(&render_params_str).unwrap();

            crate::mandelbrot_search::mandelbrot_search_render(
                &params,
                &crate::file_io::build_output_path("mandelbrot_search"),
            )
            .unwrap();
        }

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
