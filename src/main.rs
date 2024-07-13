mod barnsley_fern;
mod chaos_game;
mod cli;
mod ddp_utils;
mod file_io;
mod histogram;
mod mandelbrot_core;
mod mandelbrot_search;
mod ode_solvers;
mod render;
mod serpinsky;

use clap::Parser;
use cli::RenderParams;

use crate::cli::{CommandsEnum, FractalRendererArgs};

fn build_params(cli_params: &cli::ParameterFilePath) -> mandelbrot_core::MandelbrotParams {
    let mut mandel_params: mandelbrot_core::MandelbrotParams = serde_json::from_str(
        &std::fs::read_to_string(&cli_params.params_path).expect("Unable to read param file"),
    )
    .unwrap();

    if let Some(trans) = &cli_params.translate {
        mandel_params.image_specification.center[0] +=
            trans[0] * mandel_params.image_specification.width;
        mandel_params.image_specification.center[1] +=
            trans[1] * mandel_params.image_specification.height();
    }

    if let Some(alpha) = cli_params.rescale {
        mandel_params.image_specification.width *= alpha
    }

    mandel_params
}

pub fn main_render(
    // TODO:  fix namespacing
    params: &RenderParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    match params {
        RenderParams::Mandelbrot(inner_params) => {
            crate::mandelbrot_core::render_mandelbrot_set(inner_params, file_prefix)
        }
        RenderParams::MandelbrotSearch(inner_params) => {
            crate::mandelbrot_search::mandelbrot_search_render(inner_params, file_prefix)
        }
        RenderParams::DrivenDampedPendulum(inner_params) => {
            crate::ddp_utils::render_driven_damped_pendulum_attractor(inner_params, file_prefix)
        }
        RenderParams::BarnsleyFern(inner_params) => {
            crate::barnsley_fern::render_barnsley_fern(inner_params, file_prefix)
        }
        RenderParams::Serpinsky(inner_params) => {
            crate::serpinsky::render_serpinsky(inner_params, file_prefix)
        }
    }
}

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();
    let datetime = file_io::date_time_string();

    match &args.command {
        Some(CommandsEnum::MandelbrotRender(params)) => {
            crate::mandelbrot_core::render_mandelbrot_set(
                &build_params(params),
                &file_io::FilePrefix {
                    directory_path: crate::file_io::build_output_path_with_date_time(
                        params,
                        "mandelbrot_render",
                        &datetime,
                    ),
                    file_base: file_io::extract_base_name(&params.params_path).to_owned(),
                },
            )
            .unwrap();
        }

        Some(CommandsEnum::DrivenDampedPendulumRender(params)) => {
            crate::ddp_utils::render_driven_damped_pendulum_attractor(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &file_io::FilePrefix {
                    directory_path: crate::file_io::build_output_path_with_date_time(
                        params,
                        "ddp_render",
                        &datetime,
                    ),
                    file_base: file_io::extract_base_name(&params.params_path).to_owned(),
                },
            )
            .unwrap();
        }

        Some(CommandsEnum::BarnsleyFernRender(params)) => {
            crate::barnsley_fern::render_barnsley_fern(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &file_io::FilePrefix {
                    directory_path: crate::file_io::build_output_path_with_date_time(
                        params,
                        "barnsley_fern",
                        &datetime,
                    ),
                    file_base: file_io::extract_base_name(&params.params_path).to_owned(),
                },
            )
            .unwrap();
        }

        Some(CommandsEnum::SerpinskyRender(params)) => {
            crate::serpinsky::render_serpinsky(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &file_io::FilePrefix {
                    directory_path: crate::file_io::build_output_path_with_date_time(
                        params,
                        "serpinsky",
                        &datetime,
                    ),
                    file_base: file_io::extract_base_name(&params.params_path).to_owned(),
                },
            )
            .unwrap();
        }

        Some(CommandsEnum::Render(params)) => {
            render(
                &serde_json::from_str(
                    &std::fs::read_to_string(&params.params_path)
                        .expect("Unable to read param file"),
                )
                .unwrap(),
                &file_io::FilePrefix {
                    directory_path: crate::file_io::build_output_path_with_date_time(
                        params, "render", &datetime, // TODO:  pass correct base name?
                    ),
                    file_base: file_io::extract_base_name(&params.params_path).to_owned(),
                },
            )
            .unwrap();
        }

        None => {
            println!("Default command (nothing specified!)");
        }
    }
}
