mod barnsley_fern;
mod chaos_game;
mod cli;
mod core;
mod ddp_utils;
mod file_io;
mod mandelbrot_core;
mod mandelbrot_search;
mod ode_solvers;
mod serpinsky;

use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::cli::{CommandsEnum, FractalRendererArgs};

#[derive(Serialize, Deserialize, Debug)]
pub enum RenderParams {
    Mandelbrot(crate::mandelbrot_core::MandelbrotParams),
    MandelbrotSearch(crate::mandelbrot_search::MandelbrotSearchParams),
    DrivenDampedPendulum(crate::ddp_utils::DrivenDampedPendulumParams),
    BarnsleyFern(crate::barnsley_fern::BarnsleyFernParams),
    Serpinsky(crate::serpinsky::SerpinskyParams),
}

pub fn render_fractal<F>(
    // TODO:  fix namespacing
    params: &RenderParams,
    file_prefix: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(&str) -> file_io::FilePrefix,
{
    match params {
        RenderParams::Mandelbrot(inner_params) => {
            crate::mandelbrot_core::render_mandelbrot_set(inner_params, &file_prefix("mendelbrot"))
        }
        RenderParams::MandelbrotSearch(inner_params) => {
            crate::mandelbrot_search::mandelbrot_search_render(
                inner_params,
                &file_prefix("mandelbrot_search"),
            )
        }
        RenderParams::DrivenDampedPendulum(inner_params) => {
            crate::ddp_utils::render_driven_damped_pendulum_attractor(
                inner_params,
                &file_prefix("DDP"),
            )
        }
        RenderParams::BarnsleyFern(inner_params) => {
            crate::barnsley_fern::render_barnsley_fern(inner_params, &file_prefix("barnsley_fern"))
        }
        RenderParams::Serpinsky(inner_params) => {
            crate::serpinsky::render_serpinsky(inner_params, &file_prefix("serpinsky"))
        }
    }
}

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();
    let datetime = file_io::date_time_string();

    match &args.command {
        Some(CommandsEnum::Render(params)) => {
            let build_file_prefix = |base_name: &str| -> file_io::FilePrefix {
                file_io::FilePrefix {
                    directory_path: crate::file_io::build_output_path_with_date_time(
                        params, base_name, &datetime,
                    ),
                    file_base: file_io::extract_base_name(&params.params_path).to_owned(),
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
