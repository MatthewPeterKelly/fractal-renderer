use crate::fractals::{
    barnsley_fern::render_barnsley_fern, common::FractalParams,
    driven_damped_pendulum::render_driven_damped_pendulum_attractor,
    mandelbrot::render_mandelbrot_set, serpinsky::render_serpinsky,
};

use crate::core::file_io::FilePrefix;

pub fn render_fractal<F>(
    params: &FractalParams,
    file_prefix: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(&str) -> FilePrefix,
{
    match params {
        FractalParams::Mandelbrot(inner_params) => {
            render_mandelbrot_set(inner_params, &file_prefix("mendelbrot"))
        }
        FractalParams::DrivenDampedPendulum(inner_params) => {
            render_driven_damped_pendulum_attractor(
                inner_params,
                &file_prefix("driven_damped_pendulum"),
            )
        }
        FractalParams::BarnsleyFern(inner_params) => {
            render_barnsley_fern(inner_params, &file_prefix("barnsley_fern"))
        }
        FractalParams::Serpinsky(inner_params) => {
            render_serpinsky(inner_params, &file_prefix("serpinsky"))
        }
    }
}
