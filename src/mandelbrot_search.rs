use iter_num_tools::grid_space;
use nalgebra::Complex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::ops::Range;

use crate::mandelbrot_core::{
    complex_range, render_mandelbrot_set, MandelbrotParams, MandelbrotSequence,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotSearchParams {
    // Parameters for each individual render:
    pub render_image_resolution: nalgebra::Complex<u32>,
    pub render_escape_radius_squared: f64,
    pub render_max_iter_count: u32,
    pub render_refinement_count: u32,
    pub render_domain_real: f64,

    // Search region:
    pub center: nalgebra::Complex<f64>,
    pub domain: nalgebra::Complex<f64>,

    // Convergence for each search query
    // Query is rejected if:
    // - threshold is reached below the min iter
    // - max iter is reached
    pub search_escape_radius_squared: f64,
    pub search_max_iter_count: u32,

    // How long to keep searching?
    pub max_num_renders: i32,
    pub max_search_count: i32,
}

impl Default for MandelbrotSearchParams {
    fn default() -> MandelbrotSearchParams {
        MandelbrotSearchParams {
            // Parameters for each individual render:
            render_image_resolution: nalgebra::Complex::<u32>::new(1920, 1080),
            render_escape_radius_squared: (4.0),
            render_max_iter_count: (550),
            render_refinement_count: (5),
            render_domain_real: (0.15),

            center: nalgebra::Complex::<f64>::new(-0.2, 0.0),
            domain: nalgebra::Complex::<f64>::new(3.0, 2.0),
            search_escape_radius_squared: (4.0),
            search_max_iter_count: (550),

            max_num_renders: (16),
            max_search_count: (10_000),
        }
    }
}

pub struct QueryResult {
    pub value: f64,
    pub point: nalgebra::Complex<f64>,
}

pub fn mandelbrot_search_render(
    params: &MandelbrotSearchParams,
    directory_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // write out the parameters too:
    let params_path = directory_path.join("search_params.json");
    std::fs::write(params_path, serde_json::to_string(params)?).expect("Unable to write file");

    let range = Complex::new(
        (params.center.re - 0.5 * params.domain.re)..(params.center.re + 0.5 * params.domain.re),
        (params.center.im - 0.5 * params.domain.im)..(params.center.im + 0.5 * params.domain.im),
    );

    let mut rng = rand::thread_rng();

    let render_dimensions = Complex::new(
        params.render_domain_real,
        params.render_domain_real * (params.render_image_resolution.re as f64)
            / (params.render_image_resolution.im as f64),
    );

    let query_resolution = nalgebra::Complex::<u32>::new(16, 9);

    for render_iter in 0..params.max_num_renders {
        let mut best_result = Option::<QueryResult>::None;

        for _ in 0..params.max_search_count {
            let test_point = sample_complex_point(&mut rng, &range);

            let test_range = complex_range(render_dimensions, test_point);

            let grid_iterator = grid_space(
                [test_range.re.start, test_range.im.start]..=[test_range.re.end, test_range.im.end],
                [query_resolution.re as usize, query_resolution.im as usize],
            );

            let mut total_value = 0.0;

            for [point_re, point_im] in grid_iterator {
                let local_point = Complex::new(point_re, point_im);
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
                image_resolution: params.render_image_resolution,
                center: query.point,
                domain_real: params.render_domain_real,
                escape_radius_squared: params.render_escape_radius_squared,
                max_iter_count: params.render_max_iter_count,
                refinement_count: params.render_refinement_count,
            };

            let render_result = render_mandelbrot_set(
                &render_params,
                directory_path,
                &format!("render_{}", render_iter),
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

fn sample_complex_point<R>(rng: &mut R, range: &Complex<Range<f64>>) -> Complex<f64>
where
    R: Rng,
{
    let real_part = rng.gen_range(range.re.clone());
    let imag_part = rng.gen_range(range.im.clone());
    Complex::new(real_part, imag_part)
}

//////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {

    use crate::mandelbrot_search::sample_complex_point;
    use more_asserts::{assert_ge, assert_le};
    use nalgebra::Complex;
    use rand::SeedableRng;

    #[test]
    fn sample_random_point_test() {
        let real_range = 0.0..1.0;
        let imag_range = 0.0..1.0;

        let range = Complex::new(real_range, imag_range);

        let mut rng = rand::rngs::StdRng::seed_from_u64(12345);

        for _ in 0..10 {
            let point = sample_complex_point(&mut rng, &range);
            assert_le!(point.re, 1.0);
            assert_ge!(point.re, -1.0);
            assert_le!(point.im, 1.0);
            assert_ge!(point.im, -1.0);
        }
    }

    // TODO:  add test for reading default parameters...
}
