use nalgebra::Complex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::ops::Range;

use crate::mandelbrot_core::{render_mandelbrot_set, MandelbrotParams, MandelbrotSequence};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotSearchParams {
    // Parameters for each individual render:
    pub render_image_resolution: nalgebra::Complex<u32>,
    pub render_escape_radius_squared: f64,
    pub render_max_iter_count: u32,
    pub render_refinement_count: u32,

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
    pub iter: f64,
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

    for render_iter in 0..params.max_num_renders {
        let mut best_result = Option::<QueryResult>::None;

        for _ in 0..params.max_search_count {
            let test_point = sample_complex_point(&mut rng, &range);

            let sequence = MandelbrotSequence::normalized_escape_count(
                &test_point,
                params.search_escape_radius_squared,
                params.search_max_iter_count,
                0, // Don't need smooth interpolation for coarse search
            );
            if let Some(iter) = sequence {
                if let Some(ref mut best_query) = best_result {
                    // we have a valid query, and a new point --> pick the best
                    if iter > best_query.iter {
                        best_query.iter = iter;
                        best_query.point = test_point;
                    }
                } else {
                    best_result = Some(QueryResult {
                        iter,
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
                domain_real: 0.05 * params.domain.re, // TODO!  Make this a param.
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
