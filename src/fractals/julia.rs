use crate::core::{
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::{ImageSpecification, Renderable},
};
use image::Rgb;
use serde::{Deserialize, Serialize};

use super::quadratic_map::{
    pixel_renderer, ColorMapParams, ConvergenceParams, QuadraticMapSequence,
    RenderableWithHistogram,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JuliaParams {
    pub image_specification: ImageSpecification,
    pub constant_term: [f64; 2],
    pub convergence_params: ConvergenceParams,
    pub color_map: ColorMapParams,
}

impl RenderableWithHistogram for JuliaParams {
    fn renderer_with_histogram(
        self,
    ) -> (
        impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync,
        Histogram,
        CumulativeDistributionFunction,
    ) {
        let convergence_params = self.convergence_params.clone();
        let constant_term = self.constant_term;
        pixel_renderer(
            &self.image_specification,
            &self.color_map,
            move |point: &[f64; 2]| {
                QuadraticMapSequence::normalized_log_escape_count(
                    point,
                    &constant_term,
                    &convergence_params,
                )
            },
            QuadraticMapSequence::log_iter_count(self.convergence_params.max_iter_count as f32),
        )
    }
}

impl Renderable for JuliaParams {
    fn renderer(self) -> impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync {
        let (renderer, _hist, _cdf) = self.renderer_with_histogram();
        renderer
    }

    fn image_specification(&self) -> &ImageSpecification {
        &self.image_specification
    }
}
