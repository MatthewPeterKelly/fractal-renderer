use pixels::Error;

use crate::{
    core::color_map_editor_ui,
    fractals::{common::FractalParams, quadratic_map::QuadraticMap},
};

/// Opens the interactive color-map editor for the given fractal parameters.
///
/// On save (Space) or window close, the updated keyframes are written back to
/// the file at `params_path`.  Returns an error only if the windowing or GPU
/// setup fails; unsupported fractal types log a message and return `Ok(())`.
pub fn edit_color_map(params: &FractalParams, params_path: String) -> Result<(), Error> {
    match params {
        FractalParams::Mandelbrot(inner) => {
            let params = *inner.clone();
            let renderer = QuadraticMap::new(params.clone());
            let path = params_path.clone();
            color_map_editor_ui::edit(renderer, move |keyframes| {
                let mut updated = params.clone();
                updated.color_map.keyframes = keyframes.to_vec();
                write_params(&path, &FractalParams::Mandelbrot(Box::new(updated)));
            })
        }

        FractalParams::Julia(inner) => {
            let params = *inner.clone();
            let renderer = QuadraticMap::new(params.clone());
            let path = params_path.clone();
            color_map_editor_ui::edit(renderer, move |keyframes| {
                let mut updated = params.clone();
                updated.color_map.keyframes = keyframes.to_vec();
                write_params(&path, &FractalParams::Julia(Box::new(updated)));
            })
        }

        other => {
            eprintln!(
                "ERROR: color_map_editor is not yet supported for {:?}.",
                std::mem::discriminant(other)
            );
            Ok(())
        }
    }
}

/// Serializes `params` to pretty-printed JSON and writes it to `path`.
fn write_params(path: &str, params: &FractalParams) {
    let json = serde_json::to_string_pretty(params).expect("Failed to serialize params");
    std::fs::write(path, json).expect("Failed to write params file");
    eprintln!("Saved color map to {path}");
}
