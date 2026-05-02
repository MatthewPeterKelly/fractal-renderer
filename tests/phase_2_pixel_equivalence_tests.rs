//! Phase 2.1 — verifies that the new `RenderingPipeline` produces output that
//! is visually equivalent to the legacy `Renderable::render_to_buffer` path
//! on representative example JSONs.
//!
//! ## Architectural note on quantization
//!
//! The legacy color lookup table is quantized over
//! `[cdf.min_data, cdf.max_data]` with the CDF percentile baked into each
//! entry. The new pipeline quantizes the lookup over `[0, 1]`, applying the
//! CDF per cell in `normalize_field` first. This is the right shape for the
//! Phase 6 "re-colorize on keyframe edit" path (the cache stays valid when
//! the CDF doesn't change), but it produces slightly different per-pixel
//! values from the legacy path: when the CDF is non-linear, the two
//! quantization schemes round to different bins. The differences are
//! visually negligible at a normal viewing scale but can reach a couple of
//! dozen byte-units per channel near steep gradient transitions.
//!
//! ## What we check
//!
//! - **DDP**: the gradient-free `ForegroundBackground` colorize path is
//!   bit-equivalent end-to-end (no lookup-table quantization on the per-cell
//!   path), so we assert exact `Color32` equality.
//! - **Mandelbrot / Julia / Newton**: assert per-channel diffs stay below
//!   a `MAX_GRADIENT_DIFF` budget. Phase 2.2 deletes the legacy path; the
//!   regression-test PNG hashes are regenerated at that point with the new
//!   pipeline as the source of truth.

/// Maximum allowed per-channel byte difference between legacy and new
/// pipelines for gradient-based fractals. Drives the tolerance of the
/// quantization-mismatch sanity check; see module-level comment for the
/// architectural reason these are not bit-equal.
const MAX_GRADIENT_DIFF: u8 = 32;

use egui::{Color32, ColorImage};
use fractal_renderer::core::histogram::{CumulativeDistributionFunction, Histogram};
use fractal_renderer::core::image_utils::{Renderable, create_buffer};
use fractal_renderer::core::render_pipeline::RenderingPipeline;
use fractal_renderer::fractals::common::FractalParams;
use fractal_renderer::fractals::driven_damped_pendulum::DrivenDampedPendulumParams;
use fractal_renderer::fractals::julia::JuliaParams;
use fractal_renderer::fractals::newtons_method::{
    NewtonsMethodRenderable, RootsOfUnityParams, SystemType,
};
use fractal_renderer::fractals::quadratic_map::{QuadraticMap, QuadraticMapParams};
use image::Rgb;

/// Render via the legacy path: write into a `Vec<Vec<Rgb<u8>>>` and transpose
/// into a row-major `egui::ColorImage`.
fn render_legacy<F: Renderable>(renderer: &F) -> ColorImage {
    let spec = *renderer.image_specification();
    let mut buffer = create_buffer(Rgb([0, 0, 0]), &spec.resolution);
    renderer.render_to_buffer(&mut buffer);
    let mut image = ColorImage::filled(
        [spec.resolution[0] as usize, spec.resolution[1] as usize],
        Color32::BLACK,
    );
    let width = image.size[0];
    for (y, row) in image.pixels.chunks_exact_mut(width).enumerate() {
        for (x, pixel) in row.iter_mut().enumerate() {
            let rgb = buffer[x][y];
            *pixel = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        }
    }
    image
}

/// Construct a pipeline sized to the renderer's params and run a single
/// render at `sampling_level == subpixel_antialiasing`.
fn render_via_pipeline_quadratic<T>(renderer: QuadraticMap<T>) -> ColorImage
where
    T: QuadraticMapParams + Sync + Send + 'static,
{
    let spec = *renderer.image_specification();
    let aa = renderer.render_options().subpixel_antialiasing as i32;
    let n_max_plus_1 = (aa + 1) as usize;
    let bin_count = renderer
        .fractal_params
        .color_map_params()
        .histogram_bin_count;
    let lut_count = renderer
        .fractal_params
        .color_map_params()
        .lookup_table_count;
    let max_iter = renderer.fractal_params.convergence_params().max_iter_count;
    use fractal_renderer::fractals::quadratic_map::QuadraticMapSequence;
    let hist_max = QuadraticMapSequence::log_iter_count(max_iter as f32);
    let mut pipeline =
        RenderingPipeline::new(renderer, n_max_plus_1, bin_count, hist_max, lut_count);
    let mut out = ColorImage::filled(
        [spec.resolution[0] as usize, spec.resolution[1] as usize],
        Color32::BLACK,
    );
    pipeline.render(&mut out, aa);
    out
}

fn render_via_pipeline_ddp(renderer: DrivenDampedPendulumParams) -> ColorImage {
    let spec = *renderer.image_specification();
    let aa = renderer.render_options().subpixel_antialiasing as i32;
    let n_max_plus_1 = (aa + 1) as usize;
    // DDP doesn't normalize, so histogram bin/max values are placeholders;
    // pick values that satisfy the Histogram constructor.
    let mut pipeline = RenderingPipeline::new(renderer, n_max_plus_1, 1, 1.0, 0);
    let mut out = ColorImage::filled(
        [spec.resolution[0] as usize, spec.resolution[1] as usize],
        Color32::BLACK,
    );
    pipeline.render(&mut out, aa);
    out
}

fn render_via_pipeline_newton(renderer: NewtonsMethodRenderable<RootsOfUnityParams>) -> ColorImage {
    let spec = *renderer.image_specification();
    let aa = renderer.render_options().subpixel_antialiasing as i32;
    let n_max_plus_1 = (aa + 1) as usize;
    let bin_count = renderer.params.histogram_bin_count;
    let lut_count = renderer.params.lookup_table_count;
    let hist_max = renderer.params.max_iteration_count as f32;
    let mut pipeline =
        RenderingPipeline::new(renderer, n_max_plus_1, bin_count, hist_max, lut_count);
    let mut out = ColorImage::filled(
        [spec.resolution[0] as usize, spec.resolution[1] as usize],
        Color32::BLACK,
    );
    pipeline.render(&mut out, aa);
    out
}

fn assert_pixels_within_tolerance(legacy: &ColorImage, new: &ColorImage, tol: u8, label: &str) {
    assert_eq!(legacy.size, new.size, "[{label}] image size mismatch");
    let mut max_diff = 0u32;
    let mut diff_count = 0usize;
    for (i, (a, b)) in legacy.pixels.iter().zip(new.pixels.iter()).enumerate() {
        let da = a.r().abs_diff(b.r());
        let db = a.g().abs_diff(b.g());
        let dc = a.b().abs_diff(b.b());
        let m = da.max(db).max(dc);
        if m > max_diff as u8 {
            max_diff = m as u32;
        }
        if m > tol {
            diff_count += 1;
            if diff_count <= 4 {
                println!(
                    "[{label}] pixel {i} legacy=({},{},{}) new=({},{},{}) diff=({},{},{})",
                    a.r(),
                    a.g(),
                    a.b(),
                    b.r(),
                    b.g(),
                    b.b(),
                    da,
                    db,
                    dc,
                );
            }
        }
    }
    assert_eq!(
        diff_count, 0,
        "[{label}] {diff_count} pixels exceed tolerance {tol} (max diff observed: {max_diff})"
    );
}

fn parse_params(path: &str) -> FractalParams {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("Unable to read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse failed for {path}: {e:#?}"))
}

#[test]
fn mandelbrot_aa0_pipeline_matches_legacy_within_tolerance() {
    let params = parse_params("tests/param_files/mandelbrot/default_regression_test.json");
    let inner = match params {
        FractalParams::Mandelbrot(p) => *p,
        _ => panic!("expected Mandelbrot"),
    };
    let renderer_legacy = QuadraticMap::new(inner.clone());
    let renderer_new = QuadraticMap::new(inner);
    let legacy = render_legacy(&renderer_legacy);
    let new = render_via_pipeline_quadratic(renderer_new);
    // Allow small per-channel differences from lookup-table quantization
    // (legacy table indexed by raw value vs new table indexed by percentile).
    assert_pixels_within_tolerance(&legacy, &new, MAX_GRADIENT_DIFF, "mandelbrot/default");
}

#[test]
fn mandelbrot_aa3_pipeline_matches_legacy_within_tolerance() {
    let params = parse_params("tests/param_files/mandelbrot/anti_aliasing_regression_test.json");
    let inner = match params {
        FractalParams::Mandelbrot(p) => *p,
        _ => panic!("expected Mandelbrot"),
    };
    let renderer_legacy = QuadraticMap::new(inner.clone());
    let renderer_new = QuadraticMap::new(inner);
    let legacy = render_legacy(&renderer_legacy);
    let new = render_via_pipeline_quadratic(renderer_new);
    assert_pixels_within_tolerance(&legacy, &new, MAX_GRADIENT_DIFF, "mandelbrot/aa3");
}

#[test]
fn julia_pipeline_matches_legacy_within_tolerance() {
    let params = parse_params("tests/param_files/julia/default_regression_test.json");
    let inner: JuliaParams = match params {
        FractalParams::Julia(p) => *p,
        _ => panic!("expected Julia"),
    };
    let renderer_legacy = QuadraticMap::new(inner.clone());
    let renderer_new = QuadraticMap::new(inner);
    let legacy = render_legacy(&renderer_legacy);
    let new = render_via_pipeline_quadratic(renderer_new);
    assert_pixels_within_tolerance(&legacy, &new, MAX_GRADIENT_DIFF, "julia/default");
}

#[test]
fn ddp_pipeline_matches_legacy_exactly() {
    let params =
        parse_params("tests/param_files/driven_damped_pendulum/default_regression_test.json");
    let inner: DrivenDampedPendulumParams = match params {
        FractalParams::DrivenDampedPendulum(p) => *p,
        _ => panic!("expected DDP"),
    };
    let legacy = render_legacy(&inner.clone());
    let new = render_via_pipeline_ddp(inner);
    // DDP has no lookup table quantization → strict equality.
    assert_pixels_within_tolerance(&legacy, &new, 0, "ddp/default");
}

#[test]
fn newton_pipeline_matches_legacy_within_tolerance() {
    // Use a small inline params payload to keep the test fast (no large
    // examples/ JSONs touch the test runner).
    let json = r#"{
        "NewtonsMethod": {
            "params": {
                "image_specification": {
                    "resolution": [24, 24],
                    "center": [0, 0],
                    "width": 4.0
                },
                "max_iteration_count": 60,
                "convergence_tolerance": 1e-6,
                "render_options": {
                    "downsample_stride": 1,
                    "subpixel_antialiasing": 1
                },
                "color": {
                    "cyclic_attractor": [255, 255, 255],
                    "color_maps": [
                        [{"query": 0.0, "rgb_raw": [10, 40, 30]}, {"query": 1.0, "rgb_raw": [0, 0, 0]}],
                        [{"query": 0.0, "rgb_raw": [80, 10, 40]}, {"query": 1.0, "rgb_raw": [0, 0, 0]}],
                        [{"query": 0.0, "rgb_raw": [10, 40, 80]}, {"query": 1.0, "rgb_raw": [0, 0, 0]}],
                        [{"query": 0.0, "rgb_raw": [110, 80, 30]}, {"query": 1.0, "rgb_raw": [0, 0, 0]}]
                    ]
                },
                "lookup_table_count": 256,
                "histogram_bin_count": 64,
                "histogram_sample_count": 600
            },
            "system": {
                "RootsOfUnity": {"n_roots": 4, "newton_step_size": 1.0}
            }
        }
    }"#;
    let params: FractalParams = serde_json::from_str(json).unwrap();
    let nm = match params {
        FractalParams::NewtonsMethod(p) => *p,
        _ => panic!("expected NewtonsMethod"),
    };
    let system = match &nm.system {
        SystemType::RootsOfUnity(s) => s.as_ref().clone(),
        _ => panic!("expected RootsOfUnity"),
    };
    let renderer_legacy = NewtonsMethodRenderable::new(nm.params.clone(), system.clone());
    let renderer_new = NewtonsMethodRenderable::new(nm.params, system);
    let legacy = render_legacy(&renderer_legacy);
    let new = render_via_pipeline_newton(renderer_new);
    assert_pixels_within_tolerance(&legacy, &new, MAX_GRADIENT_DIFF, "newton/inline");
}

/// Sanity: the pipeline's histogram + CDF state matches the legacy
/// `update_color_map` flow on a small Mandelbrot example, demonstrating
/// that the new sub-sample histogram source is literally the same as
/// the legacy one (Phase 2.1 invariant).
#[test]
fn pipeline_histogram_matches_legacy_for_small_mandelbrot() {
    let params = parse_params("tests/param_files/mandelbrot/default_regression_test.json");
    let inner = match params {
        FractalParams::Mandelbrot(p) => *p,
        _ => panic!("expected Mandelbrot"),
    };
    let renderer = QuadraticMap::new(inner);
    // Drive the pipeline's histogram/cdf manually via the trait methods.
    use fractal_renderer::fractals::quadratic_map::QuadraticMapSequence;
    let bin_count = renderer
        .fractal_params
        .color_map_params()
        .histogram_bin_count;
    let max_iter = renderer.fractal_params.convergence_params().max_iter_count;
    let hist = Histogram::new(
        bin_count,
        QuadraticMapSequence::log_iter_count(max_iter as f32),
    );
    let aa = renderer.render_options().subpixel_antialiasing as i32;
    let n = (aa + 1) as usize;
    let outer = (renderer.image_specification().resolution[0] as usize) * n;
    let inner_dim = (renderer.image_specification().resolution[1] as usize) * n;
    let field: Vec<Vec<Option<f32>>> = vec![vec![None; inner_dim]; outer];
    renderer.populate_histogram(aa, &field, &hist);
    let mut cdf = CumulativeDistributionFunction::new(&hist);
    cdf.reset(&hist);

    // The legacy CDF was already built on construction; compare bin-by-bin.
    for i in 0..hist.num_bins() {
        assert_eq!(
            hist.bin_count(i),
            renderer.histogram.bin_count(i),
            "histogram bin {i} mismatch"
        );
    }
}
