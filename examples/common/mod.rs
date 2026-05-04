use std::path::{Path, PathBuf};

// Note:  all of these functions are marked dead_code because they are only used in example binaries.

#[allow(unused_imports)]
use egui::{Color32, ColorImage};
#[allow(dead_code)]
use fractal_renderer::{
    cli::{color_swatch::generate_color_swatch, explore::explore_fractal, render::render_fractal},
    core::{
        color_map_editor_ui::run_color_editor,
        file_io::FilePrefix,
        image_utils::{Renderable, field_upsample_factor},
        render_pipeline::RenderingPipeline,
    },
    fractals::common::FractalParams,
};

#[allow(dead_code)]
pub fn build_output_path(project: &str) -> std::path::PathBuf {
    let directory_path: PathBuf = ["out", project].iter().collect();
    std::fs::create_dir_all(&directory_path).unwrap();
    directory_path
}

#[allow(dead_code)]
pub fn explore_example_from_string(example_name: &str) {
    let params_path = example_params_path(example_name);

    let json_text = read_params_file_or_panic(example_name, &params_path);

    let fractal_params = parse_params_json_or_panic(example_name, &params_path, &json_text);

    explore_fractal(
        &fractal_params,
        FilePrefix {
            directory_path: build_output_path(example_name),
            file_base: String::from("result"),
        },
    )
    .unwrap();
}

#[allow(dead_code)]
pub fn render_example_from_string(example_name: &str) {
    let params_path = example_params_path(example_name);

    let json_text = read_params_file_or_panic(example_name, &params_path);

    let fractal_params = parse_params_json_or_panic(example_name, &params_path, &json_text);

    render_fractal(
        &fractal_params,
        FilePrefix {
            directory_path: build_output_path(example_name),
            file_base: String::from("result"),
        },
    )
    .unwrap_or_else(|e| {
        panic!(
            "render_fractal failed for example '{}': {}",
            example_name, e
        )
    });
}

#[allow(dead_code)]
pub fn color_editor_example_from_string(example_name: &str) {
    let params_path = example_params_path(example_name);
    let json_text = read_params_file_or_panic(example_name, &params_path);
    let fractal_params: FractalParams =
        parse_params_json_or_panic(example_name, &params_path, &json_text);

    // Early prototype. Eventually this will support the same set of fractal
    // types as `explore`. Renders through the new `RenderingPipeline` and
    // converts the row-major `ColorImage` into the column-major `Vec<Vec<Rgb>>`
    // shape `run_color_editor` still expects.
    let (keyframes, preview_buffer, resolution) = match fractal_params {
        FractalParams::Mandelbrot(params) => {
            // The unified ColorMap stores per-gradient keyframes; Mandelbrot
            // always has exactly one gradient, so index 0 is safe here.
            let kf = params.color_map.color.gradients[0].clone();
            let resolution = params.image_specification.resolution;
            let renderer = (*params).clone();
            let n_max_plus_1 = field_upsample_factor(renderer.render_options().sampling_level);
            let bin_count = renderer.histogram_bin_count();
            let hist_max = renderer.histogram_max_value();
            let lut_count = renderer.lookup_table_count();
            let sampling_level = renderer.render_options().sampling_level;
            let mut pipeline =
                RenderingPipeline::new(renderer, n_max_plus_1, bin_count, hist_max, lut_count);
            let mut color_image = ColorImage::filled(
                [resolution[0] as usize, resolution[1] as usize],
                Color32::BLACK,
            );
            pipeline.render(&mut color_image, sampling_level);
            let width = color_image.size[0];
            let height = resolution[1] as usize;
            let mut buf = vec![vec![image::Rgb([0u8, 0, 0]); height]; width];
            for (x, col) in buf.iter_mut().enumerate() {
                for (y, cell) in col.iter_mut().enumerate() {
                    let c = color_image.pixels[y * width + x];
                    *cell = image::Rgb([c.r(), c.g(), c.b()]);
                }
            }
            (kf, buf, resolution)
        }
        _ => panic!(
            "color_editor_example '{}' requires Mandelbrot params",
            example_name
        ),
    };

    run_color_editor(preview_buffer, keyframes, resolution).unwrap();
}

fn example_params_path(example_name: &str) -> PathBuf {
    PathBuf::from("examples")
        .join(example_name)
        .join("params.json")
}

fn read_params_file_or_panic(example_name: &str, params_path: &Path) -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));

    // Works even if the file doesn't exists, enabling diagnostics.
    let abs_attempt = std::path::absolute(params_path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unable to absolutize: {e}>"));

    let exists = params_path.exists();
    let is_file = params_path.is_file();

    std::fs::read_to_string(params_path).unwrap_or_else(|e| {
        panic!(
            "Unable to read params file for example.\n\
             - example_name: {example_name}\n\
             - attempted path (relative): {}\n\
             - attempted path (absolute): {abs_attempt}\n\
             - current_dir: {cwd}\n\
             - exists?: {exists}\n\
             - is_file?: {is_file}\n\
             - io error: {e}\n\
             Hint: this path is resolved relative to the process working directory. \
             If you run the binary directly, the cwd may differ from `cargo run`.",
            params_path.display(),
        )
    })
}

fn parse_params_json_or_panic<T>(example_name: &str, params_path: &Path, json_text: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str::<T>(json_text).unwrap_or_else(|e| {
        let preview_len = json_text.len().min(200);
        let preview = &json_text[..preview_len];

        panic!(
            "Unable to parse params.json for example.\n\
             - example_name: {example_name}\n\
             - path: {}\n\
             - error: {e}\n\
             - file length: {} bytes\n\
             - first {preview_len} bytes preview:\n\
             {preview}",
            params_path.display(),
            json_text.len(),
        )
    })
}

#[allow(dead_code)]
pub fn color_swatch_example_from_string(example_name: &str) {
    let params_name = String::from("examples/") + example_name + &String::from("/params.json");

    generate_color_swatch(
        &params_name,
        FilePrefix {
            directory_path: build_output_path(example_name),
            file_base: String::from("result"),
        },
    );
}
