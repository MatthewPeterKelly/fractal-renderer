use crate::core::image_utils::ImageSpecification;
use serde::{Deserialize, Serialize};

use super::quadratic_map::{
    ColorMapParams, ConvergenceParams, QuadraticMapParams, QuadraticMapSequence,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JuliaParams {
    pub image_specification: ImageSpecification,
    pub constant_term: [f64; 2],
    pub convergence_params: ConvergenceParams,
    pub color_map: ColorMapParams,
}

impl QuadraticMapParams for JuliaParams {
    fn image_specification(&self) -> &ImageSpecification {
        &self.image_specification
    }

    fn set_image_specification(&mut self, image_specification: ImageSpecification) {
        self.image_specification = image_specification;
    }

    fn convergence_params(&self) -> &ConvergenceParams {
        &self.convergence_params
    }

    fn color_map(&self) -> &ColorMapParams {
        &self.color_map
    }

    fn normalized_log_escape_count(&self, point: &[f64; 2]) -> Option<f32> {
        QuadraticMapSequence::normalized_log_escape_count(
            point,
            &self.constant_term,
            &self.convergence_params,
        )
    }
}
