use serde::{Deserialize, Serialize};

use crate::core::image_utils::{ImageSpecification, RenderOptions};

/// Used to interpolate between two color values based on the iterations
/// required for the Newton-Raphson method to converge to a root.
/// Query values of 0 map to `iteration_limits[0]` and values of 1 map to
/// `iteration_limits[1]`. The `value` of zero corresponds to the common
/// background color, while a `value` of one corresponds to the foreground
/// color associated with the root that the iteration converges to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct GrayscaleMapKeyFrame {
    pub query: f32,
    pub value: f32,
}

pub enum ComplexFunctionType {
    RootsOfUnity,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RootsOfUnityParams {
    pub function_type: ComplexFunctionType,
    pub image_specification: ImageSpecification,
    pub iteration_limits: [u32; 2], // [min, max]
    pub convergence_tolerance: f64,
    pub render_options: RenderOptions,
    pub background_color_rgb: [u8; 3],
    pub root_colors_rgb: Vec<[u8; 3]>,
    pub grayscale_keyframes: Vec<GrayscaleMapKeyFrame>,
}
