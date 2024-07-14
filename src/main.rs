mod chaos_game;
mod cli;
mod core;
mod fractals;
mod mandelbrot_search;
mod serpinsky;

use core::file_io::{
    build_output_path_with_date_time, date_time_string, extract_base_name, FilePrefix,
};


use clap::Parser;
use fractals::{barnsley_fern::{render_barnsley_fern, BarnsleyFernParams}, driven_damped_pendulum::{render_driven_damped_pendulum_attractor, DrivenDampedPendulumParams}, mandelbrot::{render_mandelbrot_set, MandelbrotParams}};
use serde::{Deserialize, Serialize};

use crate::cli::{CommandsEnum, FractalRendererArgs};

#[derive(Serialize, Deserialize, Debug)]
pub enum RenderParams {
    Mandelbrot(MandelbrotParams),
    MandelbrotSearch(crate::mandelbrot_search::MandelbrotSearchParams),
    DrivenDampedPendulum(DrivenDampedPendulumParams),
    BarnsleyFern(BarnsleyFernParams),
    Serpinsky(crate::serpinsky::SerpinskyParams),
}

pub fn render_fractal<F>(
    // TODO:  fix namespacing
    params: &RenderParams,
    file_prefix: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(&str) -> FilePrefix,
{
    match params {
        RenderParams::Mandelbrot(inner_params) => {
            render_mandelbrot_set(inner_params, &file_prefix("mendelbrot"))
        }
        RenderParams::MandelbrotSearch(inner_params) => {
            crate::mandelbrot_search::mandelbrot_search_render(
                inner_params,
                &file_prefix("mandelbrot_search"),
            )
        }
        RenderParams::DrivenDampedPendulum(inner_params) => {
            render_driven_damped_pendulum_attractor(
                inner_params,
                &file_prefix("driven_damped_pendulum"),
            )
        }
        RenderParams::BarnsleyFern(inner_params) => {
            render_barnsley_fern(inner_params, &file_prefix("barnsley_fern"))
        }
        RenderParams::Serpinsky(inner_params) => {
            crate::serpinsky::render_serpinsky(inner_params, &file_prefix("serpinsky"))
        }
    }
}

fn main() {
    let args: FractalRendererArgs = FractalRendererArgs::parse();
    let datetime = date_time_string();

    match &args.command {
        Some(CommandsEnum::Render(params)) => {
            let build_file_prefix = |base_name: &str| -> FilePrefix {
                FilePrefix {
                    directory_path: build_output_path_with_date_time(params, base_name, &datetime),
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
