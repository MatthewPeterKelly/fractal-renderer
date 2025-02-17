use std::cmp::max;

use crate::core::image_utils::{ImageSpecification, RenderOptions, SpeedOptimizer};
use serde::{Deserialize, Serialize};

use super::quadratic_map::{
    ColorMapParams, ConvergenceParams, QuadraticMapParams, QuadraticMapSequence,
};

pub struct MandelbrotReferenceCache {
    pub histogram_sample_count: usize,
    pub max_iter_count: u32,
    pub downsample_stride: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MandelbrotParams {
    pub image_specification: ImageSpecification,
    pub convergence_params: ConvergenceParams,
    pub color_map: ColorMapParams,
    pub render_options: RenderOptions,
}

const ZERO_INITIAL_POINT: [f64; 2] = [0.0, 0.0];

impl QuadraticMapParams for MandelbrotParams {
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

    fn render_options(&self) -> &RenderOptions {
        &self.render_options
    }

    fn normalized_log_escape_count(&self, point: &[f64; 2]) -> Option<f32> {
        QuadraticMapSequence::normalized_log_escape_count(
            &ZERO_INITIAL_POINT,
            point,
            &self.convergence_params,
        )
    }
}

impl SpeedOptimizer for MandelbrotParams {
    type ReferenceCache = MandelbrotReferenceCache;

    fn reference_cache(&self) -> Self::ReferenceCache {
        MandelbrotReferenceCache {
            histogram_sample_count: self.color_map.histogram_sample_count,
            max_iter_count: self.convergence_params.max_iter_count,
            downsample_stride: self.render_options.downsample_stride,
        }
    }

    fn set_speed_optimization_level(&mut self, level: u32, cache: &Self::ReferenceCache) {
        let scale = 1.0 / (2u32.pow(level) as f64);
        self.color_map.histogram_sample_count =
            max(512, cache.histogram_sample_count * scale as usize);
        self.convergence_params.max_iter_count = max(128, cache.max_iter_count * scale as u32);
        self.render_options.downsample_stride = cache.downsample_stride + (level as usize);
    }
}
