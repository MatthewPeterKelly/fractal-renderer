

use fractals::{barnsley_fern::{render_barnsley_fern, BarnsleyFernParams}, driven_damped_pendulum::{render_driven_damped_pendulum_attractor, DrivenDampedPendulumParams}, mandelbrot::{render_mandelbrot_set, MandelbrotParams}, serpinsky::{render_serpinsky, SerpinskyParams}};
use serde::{Deserialize, Serialize};

use crate::{core::file_io::FilePrefix, fractals};

#[derive(Serialize, Deserialize, Debug)]
pub enum RenderParams {
    Mandelbrot(MandelbrotParams),
    MandelbrotSearch(crate::mandelbrot_search::MandelbrotSearchParams),
    DrivenDampedPendulum(DrivenDampedPendulumParams),
    BarnsleyFern(BarnsleyFernParams),
    Serpinsky(SerpinskyParams),
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
            render_serpinsky(inner_params, &file_prefix("serpinsky"))
        }
    }
}
