use crate::{chaos_game, file_io, render};
use rand::distributions::{Distribution, Uniform};
use rand::Rng;
use serde::{Deserialize, Serialize};

/**
 * Complete set of parameters that are fed in from the JSON for the Serpinsky Fern fractal.
 */
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerpinskyParams {
    pub fit_image: render::FitImage,
    pub sample_count: u32,
    pub background_color_rgba: [u8; 4],
    pub vertex_colors: Vec<[u8; 4]>,
}

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
        _ => unreachable!(), // This case will never occur
    };
    1.0 / (1.0 + alpha)
}

struct SampleGenerator {
    distribution: Uniform<usize>, // which vertext to jump to?
    vertices: Vec<nalgebra::Vector2<f64>>,
    colors: Vec<image::Rgba<u8>>,
    ratio: f64,
}

impl SampleGenerator {
    pub fn regular_polygon(
        vertex_colors: &Vec<[u8; 4]>,
        vertices: &[nalgebra::Vector2<f64>],
    ) -> SampleGenerator {
        assert_eq!(vertex_colors.len(), vertices.len());
        SampleGenerator {
            distribution: Uniform::from(0..vertex_colors.len()),
            vertices: vertices.to_vec(),
            colors: vertex_colors
                .iter()
                .map(|&color| image::Rgba(color))
                .collect(),
            ratio: optimal_contraction_ratio(vertex_colors.len()),
        }
    }

    pub fn next<R: Rng>(
        &self,
        rng: &mut R,

        prev_sample: &nalgebra::Vector2<f64>,
    ) -> chaos_game::ColoredPoint {
        let vertex_index = self.distribution.sample(rng);
        let selected_vertex = self.vertices[vertex_index];
        let next_point = self.ratio * selected_vertex + (1.0 - self.ratio) * prev_sample;
        let next_color = self.colors[vertex_index];
        chaos_game::ColoredPoint {
            point: next_point,
            color: next_color,
        }
    }
}

/**
 * Called by main, used to render the fractal using the above data structures.
 */
pub fn render_serpinsky(
    params: &SerpinskyParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let vertices = polygon_verticies(params.vertex_colors.len());
    let mut sample_point = vertices[0];
    let mut rng = rand::thread_rng();
    let generator = SampleGenerator::regular_polygon(&params.vertex_colors, &vertices);

    let mut distribution = || {
        let next_colored_point = generator.next(&mut rng, &sample_point);
        sample_point = next_colored_point.point;
        next_colored_point
    };

    chaos_game::render(
        image::Rgba(params.background_color_rgba),
        &mut distribution,
        params.sample_count,
        &params
            .fit_image
            .image_specification(&render::ViewRectangle::from_vertices(&vertices)),
        file_prefix,
        &serde_json::to_string(params)?,
    )
}

#[cfg(test)]
mod tests {
    use crate::serpinsky::optimal_contraction_ratio;
    use approx::assert_relative_eq;

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
