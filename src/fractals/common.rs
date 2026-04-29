use serde::{Deserialize, Serialize};

use super::{
    barnsley_fern::BarnsleyFernParams, driven_damped_pendulum::DrivenDampedPendulumParams,
    julia::JuliaParams, mandelbrot::MandelbrotParams, newtons_method::NewtonsMethodParams,
    serpinsky::SerpinskyParams,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum FractalParams {
    Mandelbrot(Box<MandelbrotParams>),
    Julia(Box<JuliaParams>),
    DrivenDampedPendulum(Box<DrivenDampedPendulumParams>),
    BarnsleyFern(Box<BarnsleyFernParams>),
    Serpinsky(Box<SerpinskyParams>),
    NewtonsMethod(Box<NewtonsMethodParams>),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON whose top-level variant is `Mandelbrot` but whose `color_map`
    /// payload uses Newton's `MultiColorMap` shape (`cyclic_attractor` +
    /// `color_maps`) must fail to parse. This documents that color-shape
    /// pairings are enforced at deserialization, not at runtime.
    #[test]
    fn mandelbrot_with_newton_shaped_color_payload_fails_to_parse() {
        let json = r#"{
            "Mandelbrot": {
                "image_specification": {
                    "resolution": [10, 10],
                    "center": [0, 0],
                    "width": 1.0
                },
                "convergence_params": {
                    "escape_radius_squared": 4.0,
                    "max_iter_count": 4,
                    "refinement_count": 0
                },
                "color_map": {
                    "color": {
                        "cyclic_attractor": [255, 255, 255],
                        "color_maps": [[
                            { "query": 0.0, "rgb_raw": [0, 0, 0] },
                            { "query": 1.0, "rgb_raw": [255, 255, 255] }
                        ]]
                    },
                    "lookup_table_count": 8,
                    "histogram_bin_count": 4,
                    "histogram_sample_count": 16
                },
                "render_options": {
                    "downsample_stride": 1,
                    "subpixel_antialiasing": 0
                }
            }
        }"#;
        let result: Result<FractalParams, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "expected parse failure for Newton-shaped color in Mandelbrot params"
        );
    }
}
