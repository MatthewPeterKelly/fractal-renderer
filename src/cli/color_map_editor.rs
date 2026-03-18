use std::any::type_name;

use pixels::Error;

use crate::{
    core::color_map_editor_ui,
    fractals::{
        common::FractalParams,
        quadratic_map::QuadraticMap,
    },
};

/// Open the interactive color-map editor for the given fractal parameters.
/// On save (spacebar) the updated keyframes are written back to `params_path`.
pub fn edit_color_map(params: &FractalParams, params_path: String) -> Result<(), Error> {
    match params {
        FractalParams::Mandelbrot(inner) => {
            let renderer = QuadraticMap::new(*inner.clone());
            let saved_params = inner.clone();
            let path = params_path.clone();
            color_map_editor_ui::edit(renderer, move |keyframes| {
                let mut updated = *saved_params.clone();
                updated.color_map.keyframes = keyframes.to_vec();
                let full = FractalParams::Mandelbrot(Box::new(updated));
                write_params(&path, &full);
            })
        }

        FractalParams::Julia(inner) => {
            let renderer = QuadraticMap::new(*inner.clone());
            let saved_params = inner.clone();
            let path = params_path.clone();
            color_map_editor_ui::edit(renderer, move |keyframes| {
                let mut updated = *saved_params.clone();
                updated.color_map.keyframes = keyframes.to_vec();
                let full = FractalParams::Julia(Box::new(updated));
                write_params(&path, &full);
            })
        }

        _ => {
            eprintln!(
                "ERROR: color_map_editor is not yet supported for `{}`.",
                type_name::<FractalParams>()
            );
            panic!();
        }
    }
}

fn write_params(path: &str, params: &FractalParams) {
    let json = serde_json::to_string_pretty(params).expect("Failed to serialize params");
    std::fs::write(path, json).expect("Failed to write params file");
    eprintln!("Saved color map to {path}");
}
