use crate::fractals::quadratic_map::{self, QuadraticMap};
use crate::fractals::{
    barnsley_fern::render_barnsley_fern, common::FractalParams,
    driven_damped_pendulum::render_driven_damped_pendulum_attractor, serpinsky::render_serpinsky,
};

use crate::core::file_io::FilePrefix;

pub fn render_fractal(
    params: &FractalParams,
    mut file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    match params {
        FractalParams::Mandelbrot(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("mandelbrot");
            quadratic_map::render(QuadraticMap::new((**inner_params).clone()), file_prefix)
        }
        FractalParams::Julia(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("julia");
            quadratic_map::render(QuadraticMap::new((**inner_params).clone()), file_prefix)
        }
        FractalParams::DrivenDampedPendulum(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("driven_damped_pendulum");
            render_driven_damped_pendulum_attractor(inner_params, file_prefix)
        }
        FractalParams::BarnsleyFern(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("barnsley_fern");
            render_barnsley_fern(inner_params, file_prefix)
        }
        FractalParams::Serpinsky(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("serpinsky");
            render_serpinsky(inner_params, file_prefix)
        }
    }
}
