use core::file_io::{
    build_output_path_with_date_time, extract_base_name, maybe_date_time_string, FilePrefix,
};

use clap::Parser;
use cli::args::{CommandsEnum, FractalRendererArgs, ParameterFilePath};
use cli::color_swatch::generate_color_swatch;
use cli::explore::explore_fractal;
use cli::render::render_fractal;
use fractals::common::FractalParams;

mod cli;
mod core;
mod fractals;

fn build_file_prefix(params: &ParameterFilePath, command_name: &str) -> FilePrefix {
    FilePrefix {
        directory_path: build_output_path_with_date_time(
            command_name,
            &maybe_date_time_string(params.date_time_out),
        ),
        file_base: extract_base_name(&params.params_path).to_owned(),
    }
}

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();

    let fractal_params = |path: &str| -> FractalParams {
        serde_json::from_str(&std::fs::read_to_string(path).expect("Unable to read param file"))
            .unwrap()
    };

    match &args.command {
        Some(CommandsEnum::Render(params)) => {
            render_fractal(
                &fractal_params(&params.params_path),
                build_file_prefix(params, "render"),
            )
            .unwrap();
        }

        Some(CommandsEnum::Explore(params)) => {
            explore_fractal(
                &fractal_params(&params.params_path),
                build_file_prefix(params, "explore"),
            )
            .unwrap();
        }

        Some(CommandsEnum::ColorSwatch(params)) => {
            generate_color_swatch(
                &params.params_path,
                build_file_prefix(params, "color_swatch"),
            );
        }
        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
