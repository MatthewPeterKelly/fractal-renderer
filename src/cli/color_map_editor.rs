use pixels::Error;

use crate::{
    core::color_map_editor_ui,
    fractals::{common::FractalParams, quadratic_map::QuadraticMap},
};

/// Opens the interactive color-map editor demo for the given fractal parameters.
///
/// Returns an error only if the windowing or GPU setup fails; unsupported
/// fractal types log a message and return `Ok(())`.
pub fn edit_color_map(params: &FractalParams) -> Result<(), Error> {
    match params {
        FractalParams::Mandelbrot(inner) => {
            let keyframes = inner.color_map.keyframes.clone();
            color_map_editor_ui::edit(QuadraticMap::new(*inner.clone()), keyframes)
        }

        FractalParams::Julia(inner) => {
            let keyframes = inner.color_map.keyframes.clone();
            color_map_editor_ui::edit(QuadraticMap::new(*inner.clone()), keyframes)
        }

        other => {
            let name = format!("{other:?}");
            let variant = name.split('(').next().unwrap_or("unknown");
            eprintln!("ERROR: color_map_editor is not yet supported for {variant}.");
            Ok(())
        }
    }
}
