use core::file_io::{build_output_path_with_date_time, extract_base_name, maybe_date_time_string, FilePrefix};

use clap::Parser;
use cli::args::{CommandsEnum, FractalRendererArgs};
use cli::render::render_fractal;

mod cli;
mod core;
mod fractals;
mod mandelbrot_search;

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();

    match &args.command {
        Some(CommandsEnum::Render(params)) => {

            let build_file_prefix = |base_name: &str| -> FilePrefix {
                FilePrefix {
                    directory_path: build_output_path_with_date_time(&params.params_path, base_name,&maybe_date_time_string(params.date_time_out)),
                    file_base: extract_base_name(&params.params_path).to_owned(),
                }
            };

            render_fractal(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                build_file_prefix,
            )
            .unwrap();
        }

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
