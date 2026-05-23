//! Core iteration helpers shared by every fractal that uses the
//! `RenderingPipeline`. These functions encapsulate the
//! anti-aliasing / block-fill traversal logic so per-fractal code only
//! needs to implement `FieldKernel::evaluate`.
//!
//! ## The sample-planner abstraction
//!
//! The pipeline owns a field buffer sized to the maximum sub-pixel
//! upsample factor `n_max_plus_1`. At runtime the regulator picks a
//! `sampling_level` that says how to populate that buffer:
//!
//! - **Positive `sampling_level = subpixel_count - 1`**: anti-aliasing.
//!   Each output pixel maps to one `n_max_plus_1 × n_max_plus_1` block of
//!   field cells; the top-left `subpixel_count × subpixel_count` corner of
//!   each block is populated, one sample per sub-pixel position.
//! - **`sampling_level == 0`**: baseline. One field cell per output pixel.
//! - **Negative `sampling_level = -(block_size - 1)`**: block-fill.
//!   `block_size × block_size` output pixels share one sample.
//!
//! [`SamplePlanner`] encodes this scheme in one place: given an outer
//! field index it returns `(pixel_index, subpixel_index)` for populated
//! positions or `None` for positions the traversal should skip. Both the
//! parallel-mutable traversal (used by `compute_raw_field`) and the
//! read-only traversal (used by `populate_histograms`) consult the same
//! planner, so the modular arithmetic lives in exactly one place and is
//! covered by its own unit tests.

use egui::{Color32, ColorImage};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator,
};
use rayon::slice::ParallelSliceMut;

use crate::core::color_map::{ColorPaletteCache, colorize_cell};
use crate::core::histogram::Histogram;
use crate::core::image_utils::{ImageSpecification, PixelMapper};

/// Domain-specific per-point evaluation. Each fractal implements exactly
/// this much of the math; anti-aliasing / block-fill iteration lives in
/// the shared helpers below, generic over `K: FieldKernel`.
pub trait FieldKernel: Sync + Send {
    /// Evaluate the scalar field at one real-space point.
    /// Returns `Some((value, color_map_index))` or `None` for "no value".
    /// `color_map_index` selects which color map (and which per-color-map
    /// histogram / CDF / LUT) the cell colorizes through.
    fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)>;
}

/// Decomposes a field outer index back into the corresponding
/// `(pixel_index, subpixel_index)` for a given sampling level, or `None`
/// for positions the traversal should skip. Owns the modular arithmetic
/// for both the anti-aliasing and block-fill paths so the two iteration
/// helpers (`compute_raw_field` and `populate_histograms`) consult one
/// implementation. `Copy` so it's cheap to capture in rayon closures.
#[derive(Copy, Clone, Debug)]
pub enum SamplePlanner {
    /// Anti-aliasing at `subpixel_count²` samples per output pixel.
    /// `subpixel_count = sampling_level + 1` (so `1` at baseline).
    AntiAliasing {
        /// Field cells per output pixel side (the maximum AA factor the
        /// field was sized for, plus one).
        n_max_plus_1: usize,
        /// Active sub-pixel resolution this render. `≤ n_max_plus_1`.
        subpixel_count: u32,
    },
    /// Block-fill (nearest-neighbor downsample): one sample per
    /// `block_size × block_size` output pixels.
    BlockFill {
        /// Field cells per output pixel side (always present for buffer
        /// arithmetic, even though block-fill has no sub-pixel grid).
        n_max_plus_1: usize,
        /// Output pixels per side of one nearest-neighbor block.
        /// `block_size = |sampling_level| + 1`.
        block_size: u32,
    },
}

impl SamplePlanner {
    /// Construct the planner appropriate for the runtime sampling level.
    pub fn new(n_max_plus_1: usize, sampling_level: i32) -> Self {
        assert!(n_max_plus_1 >= 1, "n_max_plus_1 must be at least 1");
        if sampling_level >= 0 {
            SamplePlanner::AntiAliasing {
                n_max_plus_1,
                subpixel_count: (sampling_level as u32) + 1,
            }
        } else {
            SamplePlanner::BlockFill {
                n_max_plus_1,
                block_size: ((-sampling_level) as u32) + 1,
            }
        }
    }

    /// Number of subpixels per output-pixel side this render. Always `1`
    /// for block-fill (the upsampled-mapper construction below treats
    /// block-fill as a degenerate AA with one subpixel per pixel).
    pub fn subpixel_count(&self) -> u32 {
        match *self {
            SamplePlanner::AntiAliasing { subpixel_count, .. } => subpixel_count,
            SamplePlanner::BlockFill { .. } => 1,
        }
    }

    /// Decompose an outer field index into the corresponding output
    /// `(pixel_index, subpixel_index)`. Returns `None` for outer indices
    /// the traversal should skip (sub-pixel positions outside the active
    /// AA grid, or block-fill positions that aren't on the stride).
    #[inline]
    pub fn decompose(&self, outer_index: usize) -> Option<(u32, u32)> {
        match *self {
            SamplePlanner::AntiAliasing {
                n_max_plus_1,
                subpixel_count,
            } => {
                // Outer index in field-buffer space splits into
                // (pixel, subpixel) by integer divmod against the field's
                // per-pixel cell count (= n_max_plus_1). The traversal
                // populates only the first `subpixel_count` slots of each
                // pixel's row/column; everything past that is dead space
                // we leave un-written for the upper-bound case where the
                // runtime AA factor < the cap baked into the buffer.
                let subpixel_index = outer_index % n_max_plus_1;
                if subpixel_index >= subpixel_count as usize {
                    return None;
                }
                let pixel_index = (outer_index / n_max_plus_1) as u32;
                Some((pixel_index, subpixel_index as u32))
            }
            SamplePlanner::BlockFill {
                n_max_plus_1,
                block_size,
            } => {
                // Block-fill samples once per `block_size`-pixel block and
                // shares the result across the block in the colorize step.
                // In field-buffer space that means one populated cell every
                // `n_max_plus_1 * block_size` outer indices, starting at 0.
                let stride = n_max_plus_1 * block_size as usize;
                if !outer_index.is_multiple_of(stride) {
                    return None;
                }
                let block_index = outer_index / stride;
                let pixel_index = (block_index * block_size as usize) as u32;
                Some((pixel_index, 0))
            }
        }
    }
}

/// Walk every populated cell of `field` in parallel by column. The closure
/// receives a mutable reference to the cell plus its decomposed
/// `(pixel_index, subpixel_index)` from the supplied planner.
///
/// Used by [`compute_raw_field`]; the iteration shape (parallel columns,
/// sequential rows) matches the existing rayon split semantics so we
/// don't fight the runtime.
pub fn par_for_each_populated_cell_mut(
    planner: SamplePlanner,
    field: &mut [Vec<Option<(f32, u32)>>],
    visit: impl Fn(&mut Option<(f32, u32)>, [u32; 2], [u32; 2]) + Sync + Send,
) {
    field.par_iter_mut().enumerate().for_each(|(outer_x, col)| {
        let Some((pixel_index_x, subpixel_index_x)) = planner.decompose(outer_x) else {
            return;
        };
        for (outer_y, cell) in col.iter_mut().enumerate() {
            if let Some((pixel_index_y, subpixel_index_y)) = planner.decompose(outer_y) {
                visit(
                    cell,
                    [pixel_index_x, pixel_index_y],
                    [subpixel_index_x, subpixel_index_y],
                );
            }
        }
    });
}

/// Read-only sibling of [`par_for_each_populated_cell_mut`]: walks every
/// populated cell of `field` in parallel by column, passing the closure
/// a shared reference to the cell plus the decomposed pixel/subpixel
/// indices. Used by [`populate_histograms`].
pub fn par_for_each_populated_cell(
    planner: SamplePlanner,
    field: &[Vec<Option<(f32, u32)>>],
    visit: impl Fn(&Option<(f32, u32)>, [u32; 2], [u32; 2]) + Sync + Send,
) {
    field.par_iter().enumerate().for_each(|(outer_x, col)| {
        let Some((pixel_index_x, subpixel_index_x)) = planner.decompose(outer_x) else {
            return;
        };
        for (outer_y, cell) in col.iter().enumerate() {
            if let Some((pixel_index_y, subpixel_index_y)) = planner.decompose(outer_y) {
                visit(
                    cell,
                    [pixel_index_x, pixel_index_y],
                    [subpixel_index_x, subpixel_index_y],
                );
            }
        }
    });
}

/// Fill the preallocated `field` with raw values produced by `kernel`.
///
/// Iteration shape comes from [`SamplePlanner`]; subpixel-to-real-space
/// math comes from constructing a `PixelMapper` against
/// `spec.upsample(subpixel_count)` and looking up the combined index
/// `pixel_index * subpixel_count + subpixel_index`. Two things follow:
///
/// 1. The sub-pixel correction is consistent — there's no longer a
///    mix of `width/(W-1)` (from `PixelMapper::map`) and `width/W` (from
///    a hand-computed `pixel_width / n`) the way the pre-cleanup code
///    used. Each subpixel slot is exactly `width/(W·n − 1)` apart, which
///    is what `PixelMapper` would produce if W were the upsampled
///    resolution all along.
/// 2. At `sampling_level == 0` and at block-fill levels, `subpixel_count`
///    is `1`, so `spec.upsample(1) == spec` and the mapper degenerates to
///    the base-resolution map — pixel hashes are invariant at those
///    levels.
///
/// Cells skipped by the planner are left untouched; the pipeline only
/// reads the populated subset on subsequent passes.
pub fn compute_raw_field<K: FieldKernel>(
    spec: &ImageSpecification,
    n_max_plus_1: usize,
    sampling_level: i32,
    kernel: &K,
    field: &mut [Vec<Option<(f32, u32)>>],
) {
    let planner = SamplePlanner::new(n_max_plus_1, sampling_level);
    let subpixel_count = planner.subpixel_count();
    let upsampled = PixelMapper::new(&spec.upsample(subpixel_count));
    par_for_each_populated_cell_mut(planner, field, |cell, pixel_index, subpixel_index| {
        let combined_x = pixel_index[0] * subpixel_count + subpixel_index[0];
        let combined_y = pixel_index[1] * subpixel_count + subpixel_index[1];
        let re = upsampled.width.map(combined_x);
        let im = upsampled.height.map(combined_y);
        *cell = kernel.evaluate([re, im]);
    });
}

/// Walk every populated cell of `field` and insert each
/// `Some((value, color_map_index))` into
/// `histograms[color_map_index % histograms.len()]`.
///
/// Callers reset the histograms first (typically via
/// `ColorPaletteCache::reset_histograms`); this function only accumulates,
/// so it's safe to call repeatedly between resets. The `&mut [Histogram]`
/// signature reflects per-render ergonomics — `Histogram::insert` itself
/// is interior-mutable, but the slice borrow ties the histograms to the
/// render that's currently filling them.
pub fn populate_histograms(
    n_max_plus_1: usize,
    sampling_level: i32,
    field: &[Vec<Option<(f32, u32)>>],
    histograms: &mut [Histogram],
) {
    let histogram_count = histograms.len();
    assert!(histogram_count > 0, "histograms slice must not be empty");
    let histograms_ref: &[Histogram] = histograms;
    let planner = SamplePlanner::new(n_max_plus_1, sampling_level);
    par_for_each_populated_cell(planner, field, |cell, _pixel_index, _subpixel_index| {
        if let Some((value, color_map_index)) = cell {
            let index = (*color_map_index as usize) % histogram_count;
            histograms_ref[index].insert(*value);
        }
    });
}

/// Walk the row-major output `egui::ColorImage`, collapsing field cells
/// into output pixels via the unified `ColorPaletteCache`.
///
/// - **Positive `sampling_level = subpixel_count - 1`**: each output pixel
///   `(px, py)` averages the `subpixel_count²` cells at
///   `field[px·n_max_plus_1 + i][py·n_max_plus_1 + j]` for
///   `i, j ∈ 0..subpixel_count`.
/// - **`sampling_level == 0`**: one cell per output pixel (the top-left
///   of each block).
/// - **Negative `sampling_level = -(block_size - 1)`**: block-fill
///   (nearest-neighbor). Every `block_size²` output-pixel block reads one
///   field cell.
///
/// CDF percentile lookup happens inside `colorize_cell`; the field stays
/// raw end-to-end. Per-pixel allocations: zero.
pub fn colorize_collapse_unified(
    cache: &ColorPaletteCache,
    field: &[Vec<Option<(f32, u32)>>],
    n_max_plus_1: usize,
    sampling_level: i32,
    out: &mut ColorImage,
) {
    let output_width = out.size[0];

    if sampling_level >= 0 {
        let subpixel_count = sampling_level as usize + 1;
        let cells_per_pixel = (subpixel_count * subpixel_count) as u32;
        out.pixels
            .par_chunks_exact_mut(output_width)
            .enumerate()
            .for_each(|(pixel_index_y, row)| {
                for (pixel_index_x, pixel) in row.iter_mut().enumerate() {
                    let mut sum = [0u32; 3];
                    for subpixel_index_x in 0..subpixel_count {
                        let cell_x = pixel_index_x * n_max_plus_1 + subpixel_index_x;
                        let col = &field[cell_x];
                        for subpixel_index_y in 0..subpixel_count {
                            let cell_y = pixel_index_y * n_max_plus_1 + subpixel_index_y;
                            let rgb = colorize_cell(cache, col[cell_y]);
                            sum[0] += rgb[0] as u32;
                            sum[1] += rgb[1] as u32;
                            sum[2] += rgb[2] as u32;
                        }
                    }
                    *pixel = Color32::from_rgb(
                        (sum[0] / cells_per_pixel) as u8,
                        (sum[1] / cells_per_pixel) as u8,
                        (sum[2] / cells_per_pixel) as u8,
                    );
                }
            });
    } else {
        let block_size = (-sampling_level) as usize + 1;
        out.pixels
            .par_chunks_exact_mut(output_width)
            .enumerate()
            .for_each(|(pixel_index_y, row)| {
                let block_y = pixel_index_y / block_size;
                let cell_y = block_y * block_size * n_max_plus_1;
                for (pixel_index_x, pixel) in row.iter_mut().enumerate() {
                    let block_x = pixel_index_x / block_size;
                    let cell_x = block_x * block_size * n_max_plus_1;
                    let rgb = colorize_cell(cache, field[cell_x][cell_y]);
                    *pixel = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color_map::{ColorMapKeyFrame, ColorPalette};

    /// Build a minimal `ColorPaletteCache` whose CDFs are pre-shaped so
    /// that percentile lookups land predictably on the color-map endpoints:
    /// inserting a single mid-bucket sample makes value `0.0` map to 0.0
    /// (low keyframe) and any value in the rightmost bin map to 1.0 (high
    /// keyframe).
    fn cache_with_unit_distribution(palette: &ColorPalette) -> ColorPaletteCache {
        let mut cache = palette.create_cache(4, 1.0, 256);
        cache.reset_histograms();
        for histogram in &cache.histograms {
            histogram.insert(0.5);
        }
        cache.refresh_after_compute_pass(palette);
        cache
    }

    /// Single-color-map palette: value at 0.0 → red, value at 1.0 → blue.
    fn red_to_blue_palette() -> ColorPalette {
        ColorPalette {
            background_color: [9, 9, 9],
            color_maps: vec![vec![
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
    /// Verifies anti-aliasing averaging works: a 2×2 block of
    /// `Some((0.0, 0))` yields the low keyframe; a block mixing `None`
    /// with `Some` averages with the background color.
    #[test]
    fn colorize_collapse_unified_aa_averaging_matches_hand_computed() {
        let palette = red_to_blue_palette();
        let cache = cache_with_unit_distribution(&palette);

        let mut field: Vec<Vec<Option<(f32, u32)>>> = vec![vec![None; 4]; 4];
        // Top-left block (px=0, py=0) ← all Some((0.0, 0)) → red.
        field[0][0] = Some((0.0, 0));
        field[0][1] = Some((0.0, 0));
        field[1][0] = Some((0.0, 0));
        field[1][1] = Some((0.0, 0));
        // Top-right block (px=1, py=0) ← all None → background color.
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
        // Bottom-left averages 2× red + 2× background:
        //  (255+255+9+9)/4=132, (0+0+9+9)/4=4, (0+0+9+9)/4=4.
        let bottom_left = pixel_at(0, 1);
        assert_eq!(bottom_left, Color32::from_rgb(132, 4, 4));
        assert_eq!(pixel_at(1, 1), Color32::from_rgb(0, 0, 255));
    }

    /// `subpixel_count = 1` (sampling_level = 0): one cell per output
    /// pixel, no averaging.
    #[test]
    fn colorize_collapse_unified_no_aa_one_cell_per_pixel() {
        let palette = red_to_blue_palette();
        let cache = cache_with_unit_distribution(&palette);

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
        let palette = red_to_blue_palette();
        let cache = cache_with_unit_distribution(&palette);

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
    /// plus a fixed color-map index.
    struct EncodingKernel {
        color_map_index: u32,
    }

    impl FieldKernel for EncodingKernel {
        fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
            let value = point[0] as f32 * 1000.0 + point[1] as f32;
            Some((value, self.color_map_index))
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

    /// Kernel that bins values by color-map index based on the x
    /// coordinate. Even pixel-x → color map 0, odd → color map 1.
    struct AlternatingKernel;

    impl FieldKernel for AlternatingKernel {
        fn evaluate(&self, point: [f64; 2]) -> Option<(f32, u32)> {
            let color_map_index = if point[0].round() as i32 % 2 == 0 {
                0
            } else {
                1
            };
            Some((point[0].abs() as f32, color_map_index))
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
    fn sample_planner_anti_aliasing_decomposes_outer_index_correctly() {
        let planner = SamplePlanner::new(4, 1); // n_max_plus_1 = 4, subpixel_count = 2

        // Within the populated 2×2 subgrid of the first 4×4 block.
        assert_eq!(planner.decompose(0), Some((0, 0)));
        assert_eq!(planner.decompose(1), Some((0, 1)));
        // Outside the active subgrid (subpixel_index ≥ subpixel_count).
        assert_eq!(planner.decompose(2), None);
        assert_eq!(planner.decompose(3), None);
        // Next pixel.
        assert_eq!(planner.decompose(4), Some((1, 0)));
        assert_eq!(planner.decompose(5), Some((1, 1)));
        assert_eq!(planner.decompose(6), None);
    }

    #[test]
    fn sample_planner_anti_aliasing_baseline_is_one_cell_per_pixel() {
        let planner = SamplePlanner::new(1, 0); // subpixel_count = 1
        // Every cell is on the populated grid; subpixel index always 0.
        for outer in 0..5 {
            assert_eq!(planner.decompose(outer), Some((outer as u32, 0)));
        }
        assert_eq!(planner.subpixel_count(), 1);
    }

    #[test]
    fn sample_planner_block_fill_skips_off_stride_indices() {
        // n_max_plus_1 = 2, block_size = 2 → stride = 4.
        let planner = SamplePlanner::new(2, -1);
        assert_eq!(planner.decompose(0), Some((0, 0)));
        assert_eq!(planner.decompose(1), None);
        assert_eq!(planner.decompose(2), None);
        assert_eq!(planner.decompose(3), None);
        // Next block: pixel_index = block_index (1) * block_size (2) = 2.
        assert_eq!(planner.decompose(4), Some((2, 0)));
        assert_eq!(planner.decompose(5), None);
        assert_eq!(planner.decompose(8), Some((4, 0)));
        assert_eq!(planner.subpixel_count(), 1);
    }

    #[test]
    fn sample_planner_traversal_visits_each_outer_index_once_for_aa() {
        // Drive the read-only traversal and assert (a) every populated
        // (outer_x, outer_y) is visited exactly once, (b) decompose
        // is consistent with what compute_raw_field would write.
        let n_max_plus_1 = 3;
        let outer_dim = 6;
        for sampling_level in [0, 1, 2] {
            let planner = SamplePlanner::new(n_max_plus_1, sampling_level);
            let field = allocate_field(outer_dim, outer_dim);
            let visits = std::sync::Mutex::new(Vec::<(usize, usize, [u32; 2], [u32; 2])>::new());
            par_for_each_populated_cell(planner, &field, |_cell, pi, si| {
                // Recover the outer indices by re-decomposing.
                // We need them to assert uniqueness — pass through via the
                // planner: pixel_index*n_max_plus_1 + subpixel_index.
                let outer_x = pi[0] as usize * n_max_plus_1 + si[0] as usize;
                let outer_y = pi[1] as usize * n_max_plus_1 + si[1] as usize;
                visits.lock().unwrap().push((outer_x, outer_y, pi, si));
            });

            let mut visits = visits.into_inner().unwrap();
            visits.sort();
            let total_unique: std::collections::BTreeSet<_> =
                visits.iter().map(|(x, y, _, _)| (*x, *y)).collect();
            assert_eq!(
                total_unique.len(),
                visits.len(),
                "sampling_level={sampling_level}: every visited (outer_x, outer_y) must be unique"
            );

            // The total visit count must equal subpixel_count² per pixel
            // times pixel-count², where pixel-count = outer_dim / n_max_plus_1.
            let subpixel_count = planner.subpixel_count() as usize;
            let pixel_count_per_side = outer_dim / n_max_plus_1;
            let expected = subpixel_count.pow(2) * pixel_count_per_side.pow(2);
            assert_eq!(visits.len(), expected, "sampling_level={sampling_level}");
        }
    }

    #[test]
    fn sample_planner_traversal_visits_match_block_fill_stride() {
        // n_max_plus_1 = 2, block_size = 2 → stride = 4 in outer space.
        let planner = SamplePlanner::new(2, -1);
        let field = allocate_field(8, 4);
        let visits = std::sync::Mutex::new(Vec::<(usize, usize)>::new());
        par_for_each_populated_cell(planner, &field, |_cell, pi, _si| {
            // For block-fill we infer outer from pixel: outer = pixel/block_size * stride.
            let block_size = 2;
            let outer_x = (pi[0] as usize / block_size) * (block_size * 2); // n_max_plus_1=2
            let outer_y = (pi[1] as usize / block_size) * (block_size * 2);
            visits.lock().unwrap().push((outer_x, outer_y));
        });
        let mut visits = visits.into_inner().unwrap();
        visits.sort();
        // Stride = 4; field is 8 wide × 4 tall in outer indices.
        // Outer-x positions: 0, 4. Outer-y positions: 0.
        // (4 isn't reached on the y axis because inner_dim=4 and stride=4
        // gives only one position 0.)
        assert_eq!(visits, vec![(0, 0), (4, 0)]);
    }

    #[test]
    fn compute_raw_field_aa_writes_only_first_n_squared_cells_per_block() {
        let spec = make_spec(2, 2, 4.0);
        let n_max_plus_1 = 3; // field is 6×6
        let mut field = allocate_field(6, 6);
        let kernel = EncodingKernel { color_map_index: 0 };

        // sampling_level = 1 → subpixel_count = 2; only the first 2×2
        // sub-grid of each 3×3 block should be populated.
        compute_raw_field(&spec, n_max_plus_1, 1, &kernel, &mut field);

        for (outer_x, col) in field.iter().enumerate() {
            for (outer_y, cell) in col.iter().enumerate() {
                let subpixel_x = outer_x % n_max_plus_1;
                let subpixel_y = outer_y % n_max_plus_1;
                if subpixel_x < 2 && subpixel_y < 2 {
                    assert!(
                        cell.is_some(),
                        "cell ({outer_x},{outer_y}) within sub-grid should be populated"
                    );
                } else {
                    assert!(
                        cell.is_none(),
                        "cell ({outer_x},{outer_y}) outside sub-grid should remain None"
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
        let kernel = EncodingKernel { color_map_index: 7 };

        compute_raw_field(&spec, n_max_plus_1, 0, &kernel, &mut field);

        for (outer_x, col) in field.iter().enumerate() {
            for (outer_y, cell) in col.iter().enumerate() {
                let is_top_left = outer_x % n_max_plus_1 == 0 && outer_y % n_max_plus_1 == 0;
                assert_eq!(
                    cell.is_some(),
                    is_top_left,
                    "cell ({outer_x},{outer_y}) populated state mismatch"
                );
                if let Some((_, color_map_index)) = cell {
                    assert_eq!(
                        *color_map_index, 7,
                        "color-map index must come from the kernel"
                    );
                }
            }
        }
    }

    #[test]
    fn compute_raw_field_block_fill_writes_one_cell_per_2x2_pixel_block() {
        let spec = make_spec(4, 2, 4.0);
        let n_max_plus_1 = 1; // field is 4×2 (no AA grid; block-fill is sparse over 1× field)
        let mut field = allocate_field(4, 2);
        let kernel = EncodingKernel { color_map_index: 0 };

        // sampling_level = -1 → block_size = 2; stride = 1 * 2 = 2.
        // Field cells (0,0), (2,0) populated iff outer_x % 2 == 0 and outer_y % 2 == 0.
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
    /// the kernel sees the same coordinates a base-resolution `PixelMapper`
    /// would compute at `sampling_level = 0`.
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
    fn compute_raw_field_baseline_pixel_coords_match_base_pixel_mapper() {
        // 4×2 image; baseline sampling so each pixel is visited exactly
        // once. At sampling_level=0 the upsampled mapper degenerates to
        // the base-resolution mapper, so the coordinates handed to the
        // kernel must agree byte-for-byte with PixelMapper::new(&spec).
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
        for pixel_index_x in 0..4u32 {
            for pixel_index_y in 0..2u32 {
                expected.push([
                    pixel_map.width.map(pixel_index_x),
                    pixel_map.height.map(pixel_index_y),
                ]);
            }
        }
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(*seen, expected);
    }

    #[test]
    fn populate_histograms_routes_by_color_map_index() {
        let spec = make_spec(4, 1, 4.0);
        let n_max_plus_1 = 1;
        let mut field = allocate_field(4, 1);
        compute_raw_field(&spec, n_max_plus_1, 0, &AlternatingKernel, &mut field);

        let mut histograms = vec![Histogram::new(4, 10.0), Histogram::new(4, 10.0)];
        for histogram in &mut histograms {
            histogram.reset();
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
        let kernel = EncodingKernel { color_map_index: 0 };
        compute_raw_field(&spec, n_max_plus_1, 1, &kernel, &mut field);

        let some_count = field
            .iter()
            .flat_map(|c| c.iter())
            .filter(|c| c.is_some())
            .count();
        // subpixel_count=2, so each 2×2 block has 4 populated cells;
        // 3×3 = 9 blocks → 36.
        assert_eq!(some_count, 9 * 4);

        let mut histograms = vec![Histogram::new(8, 10000.0)];
        for histogram in &mut histograms {
            histogram.reset();
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
        for histogram in &mut histograms {
            histogram.reset();
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
