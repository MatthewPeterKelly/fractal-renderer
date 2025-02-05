//! Benchmark to measure (and then optimize) the implementation
//! of the histogram generation for the Mandelbrot set.
//! This in practice will exercise both the histogram and mandelbrot
//! core evaluation.
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fractal_renderer::fractals::{
    mandelbrot::MandelbrotParams,
    quadratic_map::{create_empty_histogram, populate_histogram},
};

pub fn import_mandelbrot_params(path: &str) -> MandelbrotParams {
    serde_json::from_str(&std::fs::read_to_string(path).expect("Unable to read param file"))
        .unwrap()
}

fn benchmark(c: &mut Criterion) {
    {
        let mandelbrot_params = import_mandelbrot_params("benches/mandelbrot_ice_fracture.json");

        let mut histogram = create_empty_histogram(&mandelbrot_params);
        c.bench_function("mandelbrot_histogram", |b| {
            b.iter(|| {
                histogram.reset();
                populate_histogram(&mandelbrot_params, &mut histogram);
                black_box(&histogram);
            });
        });
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
