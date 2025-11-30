use std::any::type_name;

use pixels::Error;

use crate::{
    core::{file_io::FilePrefix, user_interface},
    fractals::{common::FractalParams, quadratic_map::QuadraticMap},
};

/**
 * Create a simple GUI window that can be used to explore a fractal.
 * Supported features:
 * -- arrow keys for pan control
 * -- W/S keys for zoom control
 * -- mouse left click to recenter the image
 * -- A/D keys to adjust pan/zoom sensitivity
 */
pub fn explore_fractal(params: &FractalParams, mut file_prefix: FilePrefix) -> Result<(), Error> {
    match params {
        FractalParams::Mandelbrot(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("mandelbrot");
            user_interface::explore(
                file_prefix,
                inner_params.image_specification,
                QuadraticMap::new(*inner_params.clone()),
            )
        }

        FractalParams::Julia(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("julia");
            user_interface::explore(
                file_prefix,
                inner_params.image_specification,
                QuadraticMap::new(*inner_params.clone()),
            )
        }

        FractalParams::DrivenDampedPendulum(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("driven_damped_pendulum");
            user_interface::explore(
                file_prefix,
                inner_params.image_specification,
                (**inner_params).clone(),
            )
        }

        _ => {
            println!(
                "ERROR: Parameter type `{}` does not yet implement the `RenderWindow` trait!  Aborting.",
                type_name::<FractalParams>()
            );
            panic!();
        }
    }
}
