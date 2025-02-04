//! Benchmark to measure (and then optimize) the implementation
//! of the histogram generation for the Mandelbrot set.
//! This in practice will exercise both the histogram and mandelbrot
//! core evaluation.
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn add_all_the_numbers() {
    let mut sum = 0;
    for i in 0..100000 {
        sum += i;
    }
    black_box(sum);
}

fn benchmark(c: &mut Criterion) {
    c.bench_function("add_all_the_numbers", |b| {
        b.iter(|| add_all_the_numbers());
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
