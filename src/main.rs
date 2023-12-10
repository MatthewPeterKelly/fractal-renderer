mod cli;
mod file_io;
mod mandelbrot_core;

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

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
