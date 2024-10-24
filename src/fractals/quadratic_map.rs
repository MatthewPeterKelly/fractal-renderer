use serde::{Deserialize, Serialize};

use crate::core::color_map::ColorMapKeyFrame;

#[derive(Serialize, Deserialize, Debug)]
pub struct ColorMapParams {
    pub keyframes: Vec<ColorMapKeyFrame>,
    pub lookup_table_count: usize,
    pub background_color_rgb: [u8; 3],
    pub histogram_bin_count: usize,
    pub histogram_sample_count: usize,
}
