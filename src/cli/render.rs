use crate::core::image_utils;
use crate::fractals::newtons_method::render_newtons_method;
use crate::fractals::{
    barnsley_fern::render_barnsley_fern,
    common::{FractalParams, ddp_snapshot_json, julia_snapshot_json, mandelbrot_snapshot_json},
    sierpinski::render_sierpinski,
};

use crate::core::file_io::FilePrefix;

pub fn render_fractal(
    params: &FractalParams,
    mut file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    match params {
        FractalParams::Mandelbrot(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("mandelbrot");
            image_utils::render(
                (**inner_params).clone(),
                file_prefix,
                mandelbrot_snapshot_json,
            )
        }
        FractalParams::Julia(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("julia");
            image_utils::render((**inner_params).clone(), file_prefix, julia_snapshot_json)
        }
        FractalParams::DrivenDampedPendulum(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("driven_damped_pendulum");
            image_utils::render((**inner_params).clone(), file_prefix, ddp_snapshot_json)
        }
        FractalParams::BarnsleyFern(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("barnsley_fern");
            render_barnsley_fern(inner_params, file_prefix)
        }
        FractalParams::Sierpinski(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("sierpinski");
            render_sierpinski(inner_params, file_prefix)
        }
        FractalParams::NewtonsMethod(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("newtons_method");
            render_newtons_method(inner_params, file_prefix)
        }
    }
}
