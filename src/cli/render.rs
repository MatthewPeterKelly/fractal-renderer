use crate::core::image_utils;
use crate::fractals::quadratic_map::QuadraticMap;
use crate::fractals::{
    barnsley_fern::render_barnsley_fern, common::FractalParams, serpinsky::render_serpinsky,
};

use crate::core::file_io::{build_output_path, FilePrefix};

// TODO:  crate-local?
// Or move to examples?
pub fn render_example_from_string(example_name: &str) {
    let params_name = String::from("examples/") + example_name + &String::from("/params.json");

    let fractal_params = serde_json::from_str(
        &std::fs::read_to_string(params_name).expect("Unable to read param file"),
    )
    .unwrap();

    render_fractal(
        &fractal_params,
        FilePrefix {
            directory_path: build_output_path(example_name),
            file_base: String::from("result"),
        },
    )
    .unwrap();
}

pub fn render_fractal(
    params: &FractalParams,
    mut file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    match params {
        FractalParams::Mandelbrot(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("mandelbrot");
            image_utils::render(QuadraticMap::new((**inner_params).clone()), file_prefix)
        }
        FractalParams::Julia(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("julia");
            image_utils::render(QuadraticMap::new((**inner_params).clone()), file_prefix)
        }
        FractalParams::DrivenDampedPendulum(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("driven_damped_pendulum");
            image_utils::render((**inner_params).clone(), file_prefix)
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
