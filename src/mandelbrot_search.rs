use crate::{core::image_utils::ImageSpecification, file_io};
use iter_num_tools::grid_space;
use nalgebra::Vector2;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::ops::Range;

use crate::mandelbrot_core::{
    complex_range, render_mandelbrot_set, MandelbrotParams, MandelbrotSequence,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotSearchParams {
    // Parameters for each individual render:
    pub render_image_resolution: nalgebra::Vector2<u32>,
    pub render_escape_radius_squared: f64,
    pub render_max_iter_count: u32,
    pub render_refinement_count: u32,
    pub render_view_scale_real: f64,
    pub render_histogram_bin_count: usize,

    // Search region:
    pub center: nalgebra::Vector2<f64>,
    pub view_scale: nalgebra::Vector2<f64>,

    // Convergence for each search query
    // Query is rejected if:
    // - threshold is reached below the min iter
    // - max iter is reached
    pub search_escape_radius_squared: f64,
    pub search_max_iter_count: u32,

    // The search metric averages the value over a grid of queries
    // Here we specify how many points to sample in that grid.
    pub query_resolution: nalgebra::Vector2<u32>,

    // How long to keep searching?
    pub max_num_renders: i32,
    pub max_search_count: i32,
}

pub struct QueryResult {
    pub value: f64,
    pub point: nalgebra::Vector2<f64>,
}

pub fn mandelbrot_search_render(
    params: &MandelbrotSearchParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    // write out the parameters too:
    let params_path = file_prefix.with_suffix("search_params.json");
    std::fs::write(params_path, serde_json::to_string(params)?).expect("Unable to write file");

    let range = Vector2::new(
        (params.center[0] - 0.5 * params.view_scale[0])
            ..(params.center[0] + 0.5 * params.view_scale[0]),
        (params.center[1] - 0.5 * params.view_scale[1])
            ..(params.center[1] + 0.5 * params.view_scale[1]),
    );

    let mut rng = rand::thread_rng();

    let render_dimensions = Vector2::new(
        params.render_view_scale_real,
        params.render_view_scale_real * (params.render_image_resolution[0] as f64)
            / (params.render_image_resolution[1] as f64),
    );

    for render_iter in 0..params.max_num_renders {
        let mut best_result = Option::<QueryResult>::None;

        for _ in 0..params.max_search_count {
            let test_point = sample_complex_point(&mut rng, &range);

            let test_range = complex_range(render_dimensions, test_point);

            let grid_iterator = grid_space(
                [test_range[0].start, test_range[1].start]..=[test_range[0].end, test_range[1].end],
                [
                    params.query_resolution[0] as usize,
                    params.query_resolution[1] as usize,
                ],
            );

            let mut total_value = 0.0;

            for [point_re, point_im] in grid_iterator {
                let local_point = Vector2::new(point_re, point_im);
                let sequence = MandelbrotSequence::normalized_escape_count(
                    &local_point,
                    params.search_escape_radius_squared,
                    params.search_max_iter_count,
                    0, // Don't need smooth interpolation for coarse search
                );
                if let Some(iter) = sequence {
                    total_value += iter;
                }
            }

            if total_value > 0.0 {
                if let Some(ref mut best_query) = best_result {
                    // we have a valid query, and a new point --> pick the best
                    if total_value > best_query.value {
                        best_query.value = total_value;
                        best_query.point = test_point;
                    }
                } else {
                    best_result = Some(QueryResult {
                        value: total_value,
                        point: test_point,
                    });
                }
            } else {
                // Nothing -- we are only searching over points outside of the set.
            }
        }

        // Render the best point that we found:
        if let Some(ref query) = best_result {
            let render_params = MandelbrotParams {
                image_specification: ImageSpecification {
                    resolution: params.render_image_resolution,
                    center: query.point,
                    width: params.render_view_scale_real,
                },
                escape_radius_squared: params.render_escape_radius_squared,
                max_iter_count: params.render_max_iter_count,
                refinement_count: params.render_refinement_count,
                histogram_bin_count: params.render_histogram_bin_count,
            };

            let render_result = render_mandelbrot_set(
                &render_params,
                &file_io::FilePrefix {
                    directory_path: file_prefix.directory_path.to_path_buf(),
                    file_base: format!("{}_render_{}", file_prefix.file_base, render_iter),
                },
            );

            match render_result {
                Ok(()) => {}
                Err(_) => {
                    println!("Error:  Failed to render!");
                    return render_result;
                }
            }
        } else {
            println!("Warning:  failed to find a valid point to render!");
        }
    }
    Ok(())
}

fn sample_complex_point<R>(rng: &mut R, range: &Vector2<Range<f64>>) -> Vector2<f64>
where
    R: Rng,
{
    let real_part = rng.gen_range(range[0].clone());
    let imag_part = rng.gen_range(range[1].clone());
    Vector2::new(real_part, imag_part)
}

//////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {

    use crate::mandelbrot_search::sample_complex_point;
    use more_asserts::{assert_ge, assert_le};
    use nalgebra::Vector2;
    use rand::SeedableRng;

    #[test]
    fn sample_random_point_test() {
        let real_range = 0.0..1.0;
        let imag_range = 0.0..1.0;

        let range = Vector2::new(real_range, imag_range);

        let mut rng = rand::rngs::StdRng::seed_from_u64(12345);

        for _ in 0..10 {
            let point = sample_complex_point(&mut rng, &range);
            assert_le!(point[0], 1.0);
            assert_ge!(point[0], -1.0);
            assert_le!(point[1], 1.0);
            assert_ge!(point[1], -1.0);
        }
    }

    // TODO:  add test for reading default parameters...
}
