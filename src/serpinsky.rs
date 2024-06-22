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
    pub vertex_colors: [[u8; 4]; 3], // 3 colors, each with 4 components (RGBA)
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

struct SampleGenerator {
    distribution: Uniform<usize>, // which vertext to jump to?
    vertices: Vec<nalgebra::Vector2<f64>>,
    colors: Vec<image::Rgba<u8>>,
}

impl SampleGenerator {
    pub fn new(vertex_colors: &[[u8; 4]; 3]) -> SampleGenerator {
        SampleGenerator {
            distribution: Uniform::from(0..3),
            vertices: polygon_verticies(3).clone(),
            colors: vertex_colors
                .iter()
                .map(|&color| image::Rgba(color))
                .collect(),
        }
    }

    pub fn next<R: Rng>(
        &self,
        rng: &mut R,
        prev_sample: &nalgebra::Vector2<f64>,
    ) -> chaos_game::ColoredPoint {
        let vertex_index = self.distribution.sample(rng);
        // Optional alpha reference: https://en.wikipedia.org/wiki/Chaos_game
        let alpha = 0.5; // Implicitly depends on hard-coded 3-side assumption

        let selected_vertex = self.vertices[vertex_index];
        let next_point = alpha * selected_vertex + (1.0 - alpha) * prev_sample;
        let next_color = self.colors[vertex_index];
        chaos_game::ColoredPoint {
            point: next_point,
            color: next_color,
        }
    }
}

/**
 * Called by main, used to render the fractal using the above data structures.
 *
 * Note:  most of this code is agnostic to the Serpinsky fractal. It could be pulled out into
 * a common library whenever the next sample-based fractal is added to the project.
 */
pub fn render_serpinsky(
    params: &SerpinskyParams,
    file_prefix: &file_io::FilePrefix,
) -> Result<(), Box<dyn std::error::Error>> {
    let vertices = polygon_verticies(3);
    let mut sample_point = vertices[0];
    let mut rng = rand::thread_rng();
    let generator = SampleGenerator::new(&params.vertex_colors);

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
