//! Benchmark for the fractal rendering pipeline. Runs `RenderingPipeline::render`
//! end-to-end (compute_raw_field → populate_histograms → CDF rebuild →
//! refresh_cache → colorize_collapse_unified) at the user's full sampling
//! level on a representative Mandelbrot example.
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use egui::{Color32, ColorImage};
use fractal_renderer::{
    core::{
        image_utils::{Renderable, field_upsample_factor},
        render_pipeline::RenderingPipeline,
    },
    fractals::mandelbrot::MandelbrotParams,
};

fn run_pipeline_render_benchmark(c: &mut Criterion, path: &str) {
    let mandelbrot_params: MandelbrotParams =
        serde_json::from_str(&std::fs::read_to_string(path).expect("Unable to read param file"))
            .unwrap();

    let renderer = mandelbrot_params;
    let resolution = renderer.image_specification().resolution;
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

    c.bench_function(path, |b| {
        b.iter(|| {
            pipeline.render(&mut color_image, sampling_level);
            black_box(&color_image);
        });
    });
}

fn benchmark(c: &mut Criterion) {
    run_pipeline_render_benchmark(c, "benches/mandelbrot_ice_fracture.json");
    run_pipeline_render_benchmark(c, "benches/mandelbrot_default.json");
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
