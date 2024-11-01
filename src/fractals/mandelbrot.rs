use crate::core::{
    histogram::{CumulativeDistributionFunction, Histogram},
    image_utils::
        ImageSpecification
    ,
};
use image::Rgb;
use serde::{Deserialize, Serialize};

use super::quadratic_map::{pixel_renderer, ColorMapParams, ConvergenceParams, QuadraticMapSequence, Renderable};

#[derive(Serialize, Deserialize, Debug)]
pub struct MandelbrotParams {
    pub image_specification: ImageSpecification,
    pub convergence_params: ConvergenceParams,
    pub color_map: ColorMapParams,
}

const ZERO_INITIAL_POINT: [f64; 2] = [0.0, 0.0];

impl Renderable for MandelbrotParams {

     fn renderer(
        &self
    ) -> (
        impl Fn(&nalgebra::Vector2<f64>) -> Rgb<u8> + std::marker::Sync,
        Histogram,
        CumulativeDistributionFunction,
    ) {
        let convergence_params = self.convergence_params.clone();
        pixel_renderer(&self.image_specification, &self.color_map,
                move |point: &[f64; 2]| {
                    QuadraticMapSequence::normalized_log_escape_count(
                        &ZERO_INITIAL_POINT,
                        point,
                        &convergence_params,
                    )
            }, QuadraticMapSequence::log_iter_count(self.convergence_params.max_iter_count as f32),
        )
    }

    fn image_specification(&self) -> &ImageSpecification {
        &self.image_specification
    }

}

