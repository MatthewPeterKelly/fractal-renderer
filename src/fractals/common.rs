use serde::{Deserialize, Serialize};

use crate::core::file_io::to_pretty_json_or_panic;

use super::{
    barnsley_fern::BarnsleyFernParams,
    driven_damped_pendulum::DrivenDampedPendulumParams,
    julia::JuliaParams,
    mandelbrot::MandelbrotParams,
    newtons_method::{CommonParams, NewtonsMethodParams, SystemType},
    sierpinski::SierpinskiParams,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum FractalParams {
    Mandelbrot(Box<MandelbrotParams>),
    Julia(Box<JuliaParams>),
    DrivenDampedPendulum(Box<DrivenDampedPendulumParams>),
    BarnsleyFern(Box<BarnsleyFernParams>),
    Sierpinski(Box<SierpinskiParams>),
    NewtonsMethod(Box<NewtonsMethodParams>),
}

/// Serialize Mandelbrot params as a reloadable, pretty-printed tagged
/// `FractalParams` snapshot (the `{"Mandelbrot": …}` shape `explore` /
/// `render` accept as input).
pub fn mandelbrot_snapshot_json(params: &MandelbrotParams) -> String {
    to_pretty_json_or_panic(&FractalParams::Mandelbrot(Box::new(params.clone())))
}

/// Serialize Julia params as a reloadable, pretty-printed tagged
/// `FractalParams` snapshot.
pub fn julia_snapshot_json(params: &JuliaParams) -> String {
    to_pretty_json_or_panic(&FractalParams::Julia(Box::new(params.clone())))
}

/// Serialize driven-damped-pendulum params as a reloadable, pretty-printed
/// tagged `FractalParams` snapshot.
pub fn ddp_snapshot_json(params: &DrivenDampedPendulumParams) -> String {
    to_pretty_json_or_panic(&FractalParams::DrivenDampedPendulum(Box::new(
        params.clone(),
    )))
}

/// Serialize Newton's-method params as a reloadable, pretty-printed tagged
/// `FractalParams` snapshot. The `system` must be supplied separately because
/// it is not part of the renderer's `Renderable::Params` (`CommonParams`); the
/// dispatch site that picked the concrete system threads it back in here.
pub fn newton_snapshot_json(system: &SystemType, params: &CommonParams) -> String {
    to_pretty_json_or_panic(&FractalParams::NewtonsMethod(Box::new(
        NewtonsMethodParams {
            params: params.clone(),
            system: system.clone(),
        },
    )))
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

    /// Parse a snapshot string and assert it is a valid, idempotently
    /// round-tripping `FractalParams` carrying the expected top-level tag.
    /// Idempotence (re-serializing the reparsed value reproduces the same
    /// JSON tree) sidesteps integer-vs-float `Value` mismatches that a raw
    /// comparison against hand-written input would hit.
    fn assert_round_trips(snapshot: &str, expected_tag: &str) -> serde_json::Value {
        let value: serde_json::Value = serde_json::from_str(snapshot).unwrap();
        assert!(
            value.get(expected_tag).is_some(),
            "snapshot is missing the `{expected_tag}` tag: {value}"
        );
        let reparsed: FractalParams = serde_json::from_str(snapshot).unwrap();
        assert_eq!(value, serde_json::to_value(&reparsed).unwrap());
        value
    }

    #[test]
    fn mandelbrot_snapshot_json_round_trips() {
        let json = r#"{"Mandelbrot":{"image_specification":{"resolution":[10,10],"center":[0,0],"width":1.0},"convergence_params":{"escape_radius_squared":4.0,"max_iter_count":50,"refinement_count":2},"color_map":{"color":{"background_color":[0,0,0],"color_maps":[[{"query":0.0,"rgb_raw":[0,255,0]},{"query":1.0,"rgb_raw":[255,0,0]}]]},"lookup_table_count":256,"histogram_bin_count":4},"render_options":{"sampling_level":1}}}"#;
        let FractalParams::Mandelbrot(inner) = serde_json::from_str(json).unwrap() else {
            panic!("expected Mandelbrot variant");
        };
        assert_round_trips(&mandelbrot_snapshot_json(&inner), "Mandelbrot");
    }

    #[test]
    fn julia_snapshot_json_round_trips() {
        let json = r#"{"Julia":{"image_specification":{"resolution":[10,10],"center":[0,0],"width":3.8},"constant_term":[-0.8,0.156],"convergence_params":{"escape_radius_squared":4.0,"max_iter_count":64,"refinement_count":2},"color_map":{"color":{"background_color":[14,14,14],"color_maps":[[{"query":0.0,"rgb_raw":[0,0,0]},{"query":1.0,"rgb_raw":[0,50,230]}]]},"lookup_table_count":256,"histogram_bin_count":4},"render_options":{"sampling_level":1}}}"#;
        let FractalParams::Julia(inner) = serde_json::from_str(json).unwrap() else {
            panic!("expected Julia variant");
        };
        assert_round_trips(&julia_snapshot_json(&inner), "Julia");
    }

    #[test]
    fn ddp_snapshot_json_round_trips() {
        let json = r#"{"DrivenDampedPendulum":{"image_specification":{"resolution":[10,10],"center":[0,0],"width":14.0},"time_phase":0.0,"n_max_period":50,"n_steps_per_period":12,"periodic_state_error_tolerance":0.05,"render_options":{"sampling_level":1},"color":{"background_color":[0,0,0],"color_maps":[[{"query":0.0,"rgb_raw":[255,255,255]},{"query":1.0,"rgb_raw":[255,255,255]}]]}}}"#;
        let FractalParams::DrivenDampedPendulum(inner) = serde_json::from_str(json).unwrap() else {
            panic!("expected DrivenDampedPendulum variant");
        };
        assert_round_trips(&ddp_snapshot_json(&inner), "DrivenDampedPendulum");
    }

    /// The Newton snapshot must re-inject the `system`, which is not part of
    /// `CommonParams` (`Renderable::Params`). A missing system would fail to
    /// deserialize; this also asserts the system content is preserved exactly.
    #[test]
    fn newton_snapshot_json_round_trips_and_preserves_system() {
        let json = r#"{"NewtonsMethod":{"params":{"image_specification":{"resolution":[10,10],"center":[0,0],"width":5.0},"max_iteration_count":250,"convergence_tolerance":1e-6,"render_options":{"sampling_level":2},"color":{"background_color":[255,255,255],"color_maps":[[{"query":0.0,"rgb_raw":[9,42,27]},{"query":1.0,"rgb_raw":[0,0,0]}]]},"lookup_table_count":512,"histogram_bin_count":512},"system":{"RootsOfUnity":{"n_roots":4,"newton_step_size":1.0}}}}"#;
        let FractalParams::NewtonsMethod(inner) = serde_json::from_str(json).unwrap() else {
            panic!("expected NewtonsMethod variant");
        };
        let snapshot = newton_snapshot_json(&inner.system, &inner.params);
        let value = assert_round_trips(&snapshot, "NewtonsMethod");
        assert_eq!(
            value["NewtonsMethod"]["system"],
            serde_json::json!({ "RootsOfUnity": { "n_roots": 4, "newton_step_size": 1.0 } }),
            "Newton snapshot dropped or altered the system"
        );
    }
}
