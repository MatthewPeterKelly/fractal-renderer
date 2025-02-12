//! Benchmark to measure (and then optimize) the implementation
//! of the histogram generation for the Mandelbrot set.
//! This in practice will exercise both the histogram and mandelbrot
//! core evaluation.
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fractal_renderer::fractals::{
    mandelbrot::MandelbrotParams,
    quadratic_map::{create_empty_histogram, populate_histogram},
};

pub fn run_quadratic_map_histogram_benchmark(c: &mut Criterion, path: &str) {
    let mandelbrot_params: MandelbrotParams =
        serde_json::from_str(&std::fs::read_to_string(path).expect("Unable to read param file"))
            .unwrap();

    let histogram = create_empty_histogram(&mandelbrot_params);
    c.bench_function(path, |b| {
        b.iter(|| {
            histogram.reset();
            populate_histogram(&mandelbrot_params, histogram.clone());
            black_box(&histogram);
        });
    });
}

fn benchmark(c: &mut Criterion) {
    run_quadratic_map_histogram_benchmark(c, "benches/mandelbrot_ice_fracture.json");
    run_quadratic_map_histogram_benchmark(c, "benches/mandelbrot_default.json");
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
