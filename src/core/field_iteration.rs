//! Core iteration helpers shared by every fractal that uses the
//! `RenderingPipeline`. These functions encapsulate the AA / block-fill
//! traversal logic so per-fractal code only needs to implement
//! `FieldKernel::evaluate`.
//!
//! ## Phase 3.1 scope
//!
//! Pure parallel-to-old machinery: the helpers live here, but no fractal
//! has been migrated to call them yet. Tests against synthetic kernels
//! gate behavior; the existing `Renderable::compute_raw_field` /
//! `populate_histogram` / `normalize_field` impls remain on the runtime
//! path. Phase 3.2 deletes the per-fractal duplicates and routes the
//! pipeline through these helpers.

use egui::{Color32, ColorImage};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator,
};
use rayon::slice::ParallelSliceMut;

use crate::core::color_map::{ColorMapCache, colorize_cell};
use crate::core::histogram::Histogram;
use crate::core::image_utils::{ImageSpecification, PixelMapper};

/// Domain-specific per-point evaluation. Each fractal implements exactly
/// this much of the math; AA / block-fill iteration lives in the shared
/// helpers below, generic over `K: FieldKernel`.
pub trait FieldKernel: Sync + Send {
    /// Evaluate the scalar field at one real-space point.
    /// Returns `Some((value, gradient_index))` or `None` for "no value".
    /// `gradient_index` selects which gradient (and which per-gradient
    /// histogram / CDF / LUT) the cell colorizes through.
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)>;
}

/// Fill the preallocated `field` with raw values produced by `kernel`.
///
/// - **Positive `sampling_level = r`**: each output pixel block (`n_max_plus_1²`
///   cells) gets the first `(r+1)²` cells populated, evaluated at subpixel
///   positions `(i / (r+1), j / (r+1))` of the pixel for `i, j ∈ 0..(r+1)`.
/// - **`sampling_level == 0`**: same as above with `r = 0` (one cell per
///   block, the top-left).
/// - **Negative `sampling_level = -m`**: block-fill. Each `(m+1) × (m+1)`
///   output-pixel block uses one shared evaluation at the top-left field
///   cell of the leftmost output pixel of the block.
///
/// Cells skipped by the traversal are left untouched; the pipeline only
/// reads the populated subset on subsequent passes.
pub fn compute_raw_field<K: FieldKernel>(
    spec: &ImageSpecification,
    n_max_plus_1: usize,
    sampling_level: i32,
    kernel: &K,
    field: &mut [Vec<Option<(f32, u32)>>],
) {
    let pixel_map = PixelMapper::new(spec);
    let pixel_width = spec.width / spec.resolution[0] as f64;
    let pixel_height = spec.height() / spec.resolution[1] as f64;

    if sampling_level >= 0 {
        let n = sampling_level as usize + 1;
        let step = 1.0 / n as f64;
        field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
            let i = outer_x % n_max_plus_1;
            if i >= n {
                return;
            }
            let px = (outer_x / n_max_plus_1) as u32;
            let re = pixel_map.width.map(px) + (i as f64) * step * pixel_width;
            for (outer_y, cell) in col.iter_mut().enumerate() {
                let j = outer_y % n_max_plus_1;
                if j >= n {
                    continue;
                }
                let py = (outer_y / n_max_plus_1) as u32;
                let im = pixel_map.height.map(py) + (j as f64) * step * pixel_height;
                *cell = kernel.evaluate([re, im]);
            }
        });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        let stride = n_max_plus_1 * block_size;
        field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
            if outer_x % stride != 0 {
                return;
            }
            let block_x = outer_x / stride;
            let px = (block_x * block_size) as u32;
            let re = pixel_map.width.map(px);
            for (outer_y, cell) in col.iter_mut().enumerate() {
                if outer_y % stride != 0 {
                    continue;
                }
                let block_y = outer_y / stride;
                let py = (block_y * block_size) as u32;
                let im = pixel_map.height.map(py);
                *cell = kernel.evaluate([re, im]);
            }
        });
    }
}

/// Walk every populated cell of `field` and insert each `Some((value, k))`
/// into `histograms[k % histograms.len()]`. The pipeline calls
/// `Histogram::reset` on each entry first; this function never resets.
///
/// Histograms use atomic interior mutability, so the per-cell `insert`
/// call only needs `&Histogram`. The `&mut [Histogram]` signature is
/// here to make the per-render reset / fill semantics explicit.
pub fn populate_histograms(
    n_max_plus_1: usize,
    sampling_level: i32,
    field: &[Vec<Option<(f32, u32)>>],
    histograms: &mut [Histogram],
) {
    let n_hists = histograms.len();
    assert!(n_hists > 0, "histograms slice must not be empty");
    let histograms: &[Histogram] = histograms;
    if sampling_level >= 0 {
        let n = sampling_level as usize + 1;
        field.par_iter().enumerate().for_each(|(outer_x, col)| {
            let i = outer_x % n_max_plus_1;
            if i >= n {
                return;
            }
            for (outer_y, cell) in col.iter().enumerate() {
                let j = outer_y % n_max_plus_1;
                if j >= n {
                    continue;
                }
                if let Some((v, k)) = cell {
                    let idx = (*k as usize) % n_hists;
                    histograms[idx].insert(*v);
                }
            }
        });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        let stride = n_max_plus_1 * block_size;
        field.par_iter().enumerate().for_each(|(outer_x, col)| {
            if outer_x % stride != 0 {
                return;
            }
            for (outer_y, cell) in col.iter().enumerate() {
                if outer_y % stride != 0 {
                    continue;
                }
                if let Some((v, k)) = cell {
                    let idx = (*k as usize) % n_hists;
                    histograms[idx].insert(*v);
                }
            }
        });
    }
}

/// Walk the row-major output `egui::ColorImage`, collapsing field cells
/// into output pixels via the unified `ColorMapCache`.
///
/// - **Positive `sampling_level = r`**: each output pixel `(px, py)` averages
///   the `(r+1)²` cells at `field[px·n_max_plus_1 + i][py·n_max_plus_1 + j]`
///   for `i, j ∈ 0..(r+1)`.
/// - **`sampling_level == 0`**: one cell per output pixel (the top-left of
///   each block).
/// - **Negative `sampling_level = -m`**: block-fill (nearest-neighbor).
///   Every `(m+1) × (m+1)` output-pixel block reads one field cell.
///
/// CDF percentile lookup happens inside `colorize_cell`; the field stays
/// raw end-to-end. Per-pixel allocations: zero.
pub fn colorize_collapse_unified(
    cache: &ColorMapCache,
    field: &[Vec<Option<(f32, u32)>>],
    n_max_plus_1: usize,
    sampling_level: i32,
    out: &mut ColorImage,
) {
    let width = out.size[0];

    if sampling_level >= 0 {
        let n = sampling_level as usize + 1;
        let count = (n * n) as u32;
        out.pixels
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(py, row)| {
                for (px, pixel) in row.iter_mut().enumerate() {
                    let mut sum = [0u32; 3];
                    for i in 0..n {
                        let cx = px * n_max_plus_1 + i;
                        let col = &field[cx];
                        for j in 0..n {
                            let cy = py * n_max_plus_1 + j;
                            let rgb = colorize_cell(cache, col[cy]);
                            sum[0] += rgb[0] as u32;
                            sum[1] += rgb[1] as u32;
                            sum[2] += rgb[2] as u32;
                        }
                    }
                    *pixel = Color32::from_rgb(
                        (sum[0] / count) as u8,
                        (sum[1] / count) as u8,
                        (sum[2] / count) as u8,
                    );
                }
            });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        out.pixels
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(py, row)| {
                let block_y = py / block_size;
                let cy = block_y * block_size * n_max_plus_1;
                for (px, pixel) in row.iter_mut().enumerate() {
                    let block_x = px / block_size;
                    let cx = block_x * block_size * n_max_plus_1;
                    let rgb = colorize_cell(cache, field[cx][cy]);
                    *pixel = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color_map::{ColorMap, ColorMapKeyFrame};
    use crate::core::histogram::CumulativeDistributionFunction;

    /// Build a minimal `ColorMapCache` whose CDFs are pre-shaped so that
    /// percentile lookups land predictably on the gradient endpoints:
    /// inserting a single mid-bucket sample makes value `0.0` map to 0.0
    /// (low keyframe) and any value in the rightmost bin map to 1.0 (high
    /// keyframe).
    fn cache_with_unit_distribution(map: &ColorMap) -> ColorMapCache {
        let mut cache = map.create_cache(4, 1.0, 256);
        for cdf in cache.cdfs.iter_mut() {
            let h = Histogram::new(4, 1.0);
            h.insert(0.5);
            *cdf = CumulativeDistributionFunction::new(&h);
        }
        cache
    }

    /// Single-keyframe-equivalent gradient: value at 0.0 → red,
    /// value at 1.0 → blue.
    fn red_to_blue_map() -> ColorMap {
        ColorMap {
            flat_color: [9, 9, 9],
            gradients: vec![vec![
                ColorMapKeyFrame {
                    query: 0.0,
                    rgb_raw: [255, 0, 0],
                },
                ColorMapKeyFrame {
                    query: 1.0,
                    rgb_raw: [0, 0, 255],
                },
            ]],
        }
    }

    /// 4×4 field of `Option<(f32, u32)>` with sampling_level=1, n=2.
    /// Verifies AA averaging works: a 2×2 block of `Some((0.0, 0))` yields
    /// the low keyframe; a block mixing `None` with `Some` averages with
    /// the flat color.
    #[test]
    fn colorize_collapse_unified_aa_averaging_matches_hand_computed() {
        let map = red_to_blue_map();
        let cache = cache_with_unit_distribution(&map);

        let mut field: Vec<Vec<Option<(f32, u32)>>> = vec![vec![None; 4]; 4];
        // Top-left block (px=0, py=0) ← all Some((0.0, 0)) → red.
        field[0][0] = Some((0.0, 0));
        field[0][1] = Some((0.0, 0));
        field[1][0] = Some((0.0, 0));
        field[1][1] = Some((0.0, 0));
        // Top-right block (px=1, py=0) ← all None → flat color.
        // Bottom-left block (px=0, py=1) ← 2 Some((0.0, 0)) + 2 None.
        field[0][2] = Some((0.0, 0));
        field[0][3] = None;
        field[1][2] = Some((0.0, 0));
        field[1][3] = None;
        // Bottom-right block (px=1, py=1) ← all Some((1.0, 0)) → blue.
        field[2][2] = Some((1.0, 0));
        field[2][3] = Some((1.0, 0));
        field[3][2] = Some((1.0, 0));
        field[3][3] = Some((1.0, 0));

        let mut out = ColorImage::filled([2, 2], Color32::BLACK);
        colorize_collapse_unified(&cache, &field, 2, 1, &mut out);

        let pixel_at = |px: usize, py: usize| out.pixels[py * 2 + px];
        assert_eq!(pixel_at(0, 0), Color32::from_rgb(255, 0, 0));
        assert_eq!(pixel_at(1, 0), Color32::from_rgb(9, 9, 9));
        // Bottom-left averages 2× red + 2× flat: (255+255+9+9)/4=132,
        // (0+0+9+9)/4=4.5→4, (0+0+9+9)/4=4.5→4.
        let bl = pixel_at(0, 1);
        assert_eq!(bl, Color32::from_rgb(132, 4, 4));
        assert_eq!(pixel_at(1, 1), Color32::from_rgb(0, 0, 255));
    }

    /// `n = 1` (sampling_level = 0): one cell per output pixel, no averaging.
    #[test]
    fn colorize_collapse_unified_no_aa_one_cell_per_pixel() {
        let map = red_to_blue_map();
        let cache = cache_with_unit_distribution(&map);

        let mut field: Vec<Vec<Option<(f32, u32)>>> = vec![vec![None; 2]; 2];
        field[0][0] = Some((0.0, 0));
        field[1][1] = Some((0.0, 0));

        let mut out = ColorImage::filled([2, 2], Color32::BLACK);
        colorize_collapse_unified(&cache, &field, 1, 0, &mut out);

        assert_eq!(out.pixels[0], Color32::from_rgb(255, 0, 0)); // (0,0)
        assert_eq!(out.pixels[1], Color32::from_rgb(9, 9, 9)); // (1,0) None
        assert_eq!(out.pixels[2], Color32::from_rgb(9, 9, 9)); // (0,1) None
        assert_eq!(out.pixels[3], Color32::from_rgb(255, 0, 0)); // (1,1)
    }

    /// Block-fill (sampling_level = -1): each 2×2 output block reads one
    /// field cell.
    #[test]
    fn colorize_collapse_unified_block_fill_shares_color_in_each_block() {
        let map = red_to_blue_map();
        let cache = cache_with_unit_distribution(&map);

        let mut field: Vec<Vec<Option<(f32, u32)>>> = vec![vec![None; 4]; 4];
        field[0][0] = Some((0.0, 0));
        field[2][0] = Some((1.0, 0));
        field[0][2] = Some((1.0, 0));
        field[2][2] = Some((0.0, 0));

        let mut out = ColorImage::filled([4, 4], Color32::BLACK);
        colorize_collapse_unified(&cache, &field, 1, -1, &mut out);

        // Top-left 2×2 block: red.
        for py in 0..2 {
            for px in 0..2 {
                assert_eq!(
                    out.pixels[py * 4 + px],
                    Color32::from_rgb(255, 0, 0),
                    "({px},{py})"
                );
            }
        }
        // Top-right 2×2 block: blue.
        for py in 0..2 {
            for px in 2..4 {
                assert_eq!(
                    out.pixels[py * 4 + px],
                    Color32::from_rgb(0, 0, 255),
                    "({px},{py})"
                );
            }
        }
    }

    /// Synthetic kernel that returns a value derived from the input point
    /// plus a fixed gradient index. The value encodes both coordinates so
    /// tests can verify which point each cell saw.
    struct EncodingKernel {
        gradient_index: u32,
    }

    impl FieldKernel for EncodingKernel {
        fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
            // Encode (re, im) into a single f32; the test recovers it.
            let value = point[0] as f32 * 1000.0 + point[1] as f32;
            Some((value, self.gradient_index))
        }
    }

    /// Kernel that returns `None` everywhere — used to verify the
    /// traversal still walks the right cells but writes are `None`.
    struct AlwaysNoneKernel;

    impl FieldKernel for AlwaysNoneKernel {
        fn evaluate(&self, _point: [f64; 2]) -> Option<(f32, u32)> {
            None
        }
    }

    /// Kernel that bins values by gradient index based on the x coordinate.
    /// Even pixel-x → gradient 0, odd → gradient 1.
    struct AlternatingKernel;

    impl FieldKernel for AlternatingKernel {
        fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
            let k = if point[0].round() as i32 % 2 == 0 {
                0
            } else {
                1
            };
            Some((point[0].abs() as f32, k))
        }
    }

    fn make_spec(width: u32, height: u32, span: f64) -> ImageSpecification {
        ImageSpecification {
            resolution: [width, height],
            center: [0.0, 0.0],
            width: span,
        }
    }

    fn allocate_field(outer: usize, inner: usize) -> Vec<Vec<Option<(f32, u32)>>> {
        (0..outer).map(|_| vec![None; inner]).collect()
    }

    #[test]
    fn compute_raw_field_aa_writes_only_first_n_squared_cells_per_block() {
        let spec = make_spec(2, 2, 4.0);
        let n_max_plus_1 = 3; // field is 6×6
        let mut field = allocate_field(6, 6);
        let kernel = EncodingKernel { gradient_index: 0 };

        // sampling_level = 1 → n = 2, only first 2×2 cells of each 3×3
        // block should be populated.
        compute_raw_field(&spec, n_max_plus_1, 1, &kernel, &mut field);

        for (outer_x, col) in field.iter().enumerate() {
            for (outer_y, cell) in col.iter().enumerate() {
                let i = outer_x % n_max_plus_1;
                let j = outer_y % n_max_plus_1;
                if i < 2 && j < 2 {
                    assert!(
                        cell.is_some(),
                        "cell ({outer_x},{outer_y}) within sub-block should be populated"
                    );
                } else {
                    assert!(
                        cell.is_none(),
                        "cell ({outer_x},{outer_y}) outside sub-block should remain None"
                    );
                }
            }
        }
    }

    #[test]
    fn compute_raw_field_baseline_writes_one_cell_per_block() {
        let spec = make_spec(3, 3, 6.0);
        let n_max_plus_1 = 2; // field is 6×6
        let mut field = allocate_field(6, 6);
        let kernel = EncodingKernel { gradient_index: 7 };

        compute_raw_field(&spec, n_max_plus_1, 0, &kernel, &mut field);

        for (outer_x, col) in field.iter().enumerate() {
            for (outer_y, cell) in col.iter().enumerate() {
                let is_top_left = outer_x % n_max_plus_1 == 0 && outer_y % n_max_plus_1 == 0;
                assert_eq!(
                    cell.is_some(),
                    is_top_left,
                    "cell ({outer_x},{outer_y}) populated state mismatch"
                );
                if let Some((_, k)) = cell {
                    assert_eq!(*k, 7, "gradient index must come from the kernel");
                }
            }
        }
    }

    #[test]
    fn compute_raw_field_block_fill_writes_one_cell_per_2x2_pixel_block() {
        let spec = make_spec(4, 2, 4.0);
        let n_max_plus_1 = 1; // field is 4×2 (no AA grid; block-fill is sparse over 1× field)
        let mut field = allocate_field(4, 2);
        let kernel = EncodingKernel { gradient_index: 0 };

        // sampling_level = -1 → block_size = 2; stride = 1 * 2 = 2.
        // Field cells (0,0), (0,*), (2,0), (2,*) populated iff outer_y % 2 == 0.
        compute_raw_field(&spec, n_max_plus_1, -1, &kernel, &mut field);

        for (outer_x, col) in field.iter().enumerate() {
            for (outer_y, cell) in col.iter().enumerate() {
                let on_stride = outer_x % 2 == 0 && outer_y % 2 == 0;
                assert_eq!(
                    cell.is_some(),
                    on_stride,
                    "({outer_x},{outer_y}) populated state should match stride logic"
                );
            }
        }
    }

    /// Records every point passed to `evaluate` so the test can verify
    /// the kernel sees the same coordinates a `PixelMapper` would compute.
    struct RecordingKernel {
        seen: std::sync::Mutex<Vec<[f64; 2]>>,
    }

    impl FieldKernel for RecordingKernel {
        fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
            self.seen.lock().unwrap().push(point);
            Some((0.0, 0))
        }
    }

    #[test]
    fn compute_raw_field_passes_correct_real_space_coords_to_kernel() {
        // 4×2 image; baseline sampling so each pixel is visited exactly once.
        let spec = make_spec(4, 2, 8.0);
        let n_max_plus_1 = 1;
        let mut field = allocate_field(4, 2);
        let kernel = RecordingKernel {
            seen: std::sync::Mutex::new(Vec::new()),
        };

        compute_raw_field(&spec, n_max_plus_1, 0, &kernel, &mut field);

        let pixel_map = PixelMapper::new(&spec);
        let mut seen = kernel.seen.lock().unwrap();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut expected: Vec<[f64; 2]> = Vec::new();
        for px in 0..4u32 {
            for py in 0..2u32 {
                expected.push([pixel_map.width.map(px), pixel_map.height.map(py)]);
            }
        }
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(*seen, expected);
    }

    #[test]
    fn populate_histograms_routes_by_gradient_index() {
        let spec = make_spec(4, 1, 4.0);
        let n_max_plus_1 = 1;
        let mut field = allocate_field(4, 1);
        compute_raw_field(&spec, n_max_plus_1, 0, &AlternatingKernel, &mut field);

        let mut histograms = vec![Histogram::new(4, 10.0), Histogram::new(4, 10.0)];
        for h in &mut histograms {
            h.reset();
        }
        populate_histograms(n_max_plus_1, 0, &field, &mut histograms);

        // The traversal hits every column once (sampling_level=0). The
        // alternating kernel routes even-x → 0, odd-x → 1, so each
        // histogram should see two entries (4 px / 2 = 2).
        let total_0: u32 = (0..4).map(|i| histograms[0].bin_count(i)).sum();
        let total_1: u32 = (0..4).map(|i| histograms[1].bin_count(i)).sum();
        assert_eq!(total_0, 2, "histogram[0] should see two even-x cells");
        assert_eq!(total_1, 2, "histogram[1] should see two odd-x cells");
    }

    #[test]
    fn populate_histograms_counts_match_some_count() {
        let spec = make_spec(3, 3, 6.0);
        let n_max_plus_1 = 2; // field is 6×6
        let mut field = allocate_field(6, 6);
        let kernel = EncodingKernel { gradient_index: 0 };
        compute_raw_field(&spec, n_max_plus_1, 1, &kernel, &mut field);

        let some_count = field
            .iter()
            .flat_map(|c| c.iter())
            .filter(|c| c.is_some())
            .count();
        // n=2, so each 2×2 block has 4 populated cells; 3×3 = 9 blocks → 36.
        assert_eq!(some_count, 9 * 4);

        let mut histograms = vec![Histogram::new(8, 10000.0)];
        for h in &mut histograms {
            h.reset();
        }
        populate_histograms(n_max_plus_1, 1, &field, &mut histograms);

        let total: u32 = (0..8).map(|i| histograms[0].bin_count(i)).sum();
        assert_eq!(total as usize, some_count);
    }

    #[test]
    fn populate_histograms_skips_none_cells() {
        let spec = make_spec(2, 2, 4.0);
        let n_max_plus_1 = 1;
        let mut field = allocate_field(2, 2);
        compute_raw_field(&spec, n_max_plus_1, 0, &AlwaysNoneKernel, &mut field);

        let mut histograms = vec![Histogram::new(4, 10.0)];
        for h in &mut histograms {
            h.reset();
        }
        populate_histograms(n_max_plus_1, 0, &field, &mut histograms);

        let total: u32 = (0..4).map(|i| histograms[0].bin_count(i)).sum();
        assert_eq!(total, 0, "no Some cells → empty histograms");
    }

    #[test]
    #[should_panic(expected = "histograms slice must not be empty")]
    fn populate_histograms_rejects_empty_slice() {
        let field = allocate_field(2, 2);
        let mut histograms: Vec<Histogram> = vec![];
        populate_histograms(1, 0, &field, &mut histograms);
    }
}
