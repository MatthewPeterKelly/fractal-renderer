use crate::core::chaos_game::{chaos_game_render, ColoredPoint};
use crate::core::file_io::{serialize_to_json_or_panic, FilePrefix};
use crate::core::image_utils::{FitImage, ViewRectangle};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/**
 * Complete set of parameters that are fed in from the JSON for the Serpinsky fractal.
 * The traditional "triangle" fractal is generalized here to work for any regular polygon.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerpinskyParams {
    pub fit_image: FitImage,
    pub sample_count: u32,
    pub rng_seed: u64,
    pub subpixel_antialiasing: u32,
    pub background_color_rgb: [u8; 3],
    pub vertex_colors_rgb: Vec<[u8; 3]>,
}

/**
 * Computes the set of polygon vertices that live on the unit circle for a polygon of `num_vertices` sides.
 */
fn polygon_verticies(num_vertices: usize) -> Vec<nalgebra::Vector2<f64>> {
    let mut vertices = Vec::with_capacity(num_vertices);
    let angle_scale = 2.0 * std::f64::consts::PI / (num_vertices as f64);

    for i in 0..num_vertices {
        let angle = angle_scale * (i as f64);
        vertices.push(nalgebra::Vector2::new(angle.sin(), angle.cos()));
    }

    vertices
}

// Optimal "jump fraction" for each sample.
// Based on: https://en.wikipedia.org/wiki/Chaos_game#cite_note-6
// Abdulaziz, Abdulrahman; Said, Judy (September 2021).
// "On the contraction ratio of iterated function systems whose attractors are Sierpinski n -gons".
// Chaos, Solitons & Fractals. 150: 111140.
fn optimal_contraction_ratio(n: usize) -> f64 {
    let n_mod_4 = n % 4;
    use std::f64::consts::PI;
    let alpha = match n_mod_4 {
        0 => (PI / n as f64).tan(),
        1 | 3 => 2.0 * (PI / (2 * n) as f64).sin(),
        2 => (PI / n as f64).sin(),
        _ => unreachable!(),
    };
    1.0 / (1.0 + alpha)
}

struct SampleGenerator {
    distribution: Uniform<usize>, // samples the next vertex to jump to
    vertices: Vec<nalgebra::Vector2<f64>>,
    colors: Vec<image::Rgb<u8>>,
    ratio: f64,
}

impl SampleGenerator {
    pub fn regular_polygon(
        vertex_colors_rgb: &[[u8; 3]],
        vertices: &[nalgebra::Vector2<f64>],
    ) -> SampleGenerator {
        assert!(!vertices.is_empty());
        assert_eq!(vertex_colors_rgb.len(), vertices.len());
        SampleGenerator {
            distribution: Uniform::from(0..vertex_colors_rgb.len()),
            vertices: vertices.to_vec(),
            colors: vertex_colors_rgb
                .iter()
                .map(|&color| image::Rgb(color))
                .collect(),
            ratio: optimal_contraction_ratio(vertex_colors_rgb.len()),
        }
    }

    pub fn next<R: Rng>(&self, rng: &mut R, prev_sample: &nalgebra::Vector2<f64>) -> ColoredPoint {
        let vertex_index = self.distribution.sample(rng);
        let selected_vertex = self.vertices[vertex_index];
        let next_point = self.ratio * selected_vertex + (1.0 - self.ratio) * prev_sample;
        let next_color = self.colors[vertex_index];
        ColoredPoint {
            point: next_point.into(),
            color: next_color,
        }
    }
}

/**
 * Called by main, used to render the fractal using the above data structures.
 */
pub fn render_serpinsky(
    params: &SerpinskyParams,
    file_prefix: FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let vertices = polygon_verticies(params.vertex_colors_rgb.len());
    let mut sample_point = vertices[0];
    let mut rng = StdRng::seed_from_u64(params.rng_seed);
    let generator = SampleGenerator::regular_polygon(&params.vertex_colors_rgb, &vertices);

    let mut distribution = || {
        let next_colored_point = generator.next(&mut rng, &sample_point);
        sample_point = next_colored_point.point.into();
        next_colored_point
    };

    serialize_to_json_or_panic(file_prefix.full_path_with_suffix(".json"), &params);

    let verticies_plain: Vec<[f64; 2]> = vertices.iter().map(|p| [p.x, p.y]).collect();

    chaos_game_render(
        image::Rgb(params.background_color_rgb),
        &mut distribution,
        params.sample_count,
        params.subpixel_antialiasing,
        &params
            .fit_image
            .image_specification(&ViewRectangle::from_vertices(&verticies_plain)),
        file_prefix,
    )
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::optimal_contraction_ratio;

    #[test]
    fn test_optimal_packing_ratio() {
        let tol = 0.005;
        assert_relative_eq!(optimal_contraction_ratio(3), 0.5, epsilon = tol);

        // Solutions from: https://en.wikipedia.org/wiki/Chaos_game
        assert_relative_eq!(optimal_contraction_ratio(5), 0.618, epsilon = tol);
        assert_relative_eq!(optimal_contraction_ratio(6), 0.667, epsilon = tol);
        assert_relative_eq!(optimal_contraction_ratio(7), 0.692, epsilon = tol);
        assert_relative_eq!(optimal_contraction_ratio(8), 0.707, epsilon = tol);
        assert_relative_eq!(optimal_contraction_ratio(9), 0.742, epsilon = tol);
        assert_relative_eq!(optimal_contraction_ratio(10), 0.764, epsilon = tol);
    }
}
