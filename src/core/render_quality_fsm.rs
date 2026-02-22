pub trait RenderQualityPolicy {
    const MAX_COMMAND: f64 = 1.0;
    const MIN_COMMAND: f64 = 0.0;

    /// @param previous_command: last render command that was completed
    /// @param measured_period: how long did that render command take to complete?
    /// @return: render quality command (0 = maximum quality; 1 = maximum speed)
    ///     out of bound commands will be clamped to [0,1]
    fn evaluate(&mut self, previous_command: f64, measured_period: f64) -> f64;

    fn clamp_command(command: f64) -> f64 {
        command.clamp(Self::MIN_COMMAND, Self::MAX_COMMAND)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct InteractiveFrameRatePolicy {
    // Mapping from the error in the target period to the delta in render command
    policy: KeyframeInterpolator<f64, f64, LinearInterpolator>,
}

impl InteractiveFrameRatePolicy {
    pub fn new(target_update_period: f64) -> Self {
        assert_gt!(target_update_period, 0.0);
        // query: measured render period
        // value: commanded change in render quality
        let keyframes: Vec<InterpolationKeyframe<f64, f64>> = vec![
            InterpolationKeyframe {
                // Rendered in "zero" time
                query: 0.0,
                value: -1.0,
            },
            InterpolationKeyframe {
                query: 0.7 * target_update_period, // a bit fast
                value: -0.03,
            },
            InterpolationKeyframe {
                query: 1.0 * target_update_period, // perfect tracking
                value: 0.0,
            },
            InterpolationKeyframe {
                query: 1.3 * target_update_period, // a bit slow
                value: 0.05,
            },
            InterpolationKeyframe {
                query: 4.0 * target_update_period,
                value: 0.1,
            },
            InterpolationKeyframe {
                query: 8.0 * target_update_period,
                value: 0.3,
            },
            InterpolationKeyframe {
                query: 20.0 * target_update_period, // super slow rendering
                value: 0.6,
            },
        ];

        Self {
            policy: KeyframeInterpolator::new(keyframes, LinearInterpolator),
        }
    }
}

impl RenderQualityPolicy for InteractiveFrameRatePolicy {
    fn evaluate(&mut self, previous_command: f64, measured_period: f64) -> f64 {
        let command_delta = self.policy.evaluate(measured_period);
        let raw_command = command_delta + previous_command;
        raw_command.clamp(Self::MIN_COMMAND, Self::MAX_COMMAND)
    }
}

#[derive(Clone, Debug)]
pub struct BackgroundFrameRatePolicy {
    target_update_period: f64,
}

impl RenderQualityPolicy for BackgroundFrameRatePolicy {
    fn evaluate(&mut self, previous_command: f64, measured_period: f64) -> f64 {
        // If we were running with a large command (very slow render), then gradually
        // approach a lower command.
        if previous_command > 0.2 {
            return 0.5 * previous_command;
        }
        // If we're still slow, then take a few steps to get back to full render quality
        if measured_period > 2.0 * self.target_update_period {
            return (previous_command - 0.08).max(Self::MIN_COMMAND);
        }
        Self::MIN_COMMAND
    }
}
///////////////////////////////////////////////////////////////////////////

use more_asserts::{assert_ge, assert_gt, assert_le};

use crate::core::interpolation::{InterpolationKeyframe, KeyframeInterpolator, LinearInterpolator};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    BeginRendering,
    Interactive,
    Background,
    Idle,
}

/// Simple FSM (finite state machine) that is used to regulate the
/// render quality command for the render pipeline. While the user
/// is actively interacting with the system, we want to hit a target
/// frame rate, even if the render quality is low. However, once the
/// user stops interacting, then we need to quickly crank up the render
/// quality regardless of frame rate. Finally, once we've rendered at
/// high quality, we should shut down the render pipeline to conserve
/// resources (no need to spin at max CPU while idle...).
#[derive(Debug, Clone)]
pub struct FiniteStateMachine<F, G>
where
    F: RenderQualityPolicy,
    G: RenderQualityPolicy,
{
    mode: Mode,                  // which mode are we in right now?
    initial_render_command: f64, // what is the command to send when we first start rendering?
    interactive_policy: F,
    background_policy: G,
    previous_interactive_render_command: f64,
}

impl<F, G> FiniteStateMachine<F, G>
where
    F: RenderQualityPolicy,
    G: RenderQualityPolicy,
{
    /// Create a new FSM for regulating the render quality.
    pub fn new(initial_command: f64, interactive_policy: F, background_policy: G) -> Self {
        assert_ge!(initial_command, 0.0);
        assert_le!(initial_command, 1.0);
        let initial_command = initial_command.clamp(0.0, 1.0);
        Self {
            mode: Mode::BeginRendering,
            initial_render_command: initial_command,
            interactive_policy,
            background_policy,
            previous_interactive_render_command: initial_command,
        }
    }

    pub fn reset(&mut self) {
        self.mode = Mode::BeginRendering;
        self.previous_interactive_render_command = self.initial_render_command;
    }

    pub fn is_idle(&self) -> bool {
        self.mode == Mode::Idle
    }

    /// @param previous_render_command: previous render command, if one has been set
    /// @param render_period: if the command has been completed, how long did it take?
    /// @param is_interactive:  is the user interacting with the fractal view port?
    pub fn render_required(
        &mut self,
        previous_render_command: Option<f64>,
        render_period: Option<f64>,
        is_interactive: bool,
    ) -> Option<f64> {
        match self.mode {
            Mode::BeginRendering => self.update_begin_rendering(is_interactive),
            Mode::Interactive => {
                self.update_interactive(previous_render_command, render_period, is_interactive)
            }
            Mode::Background => {
                self.update_background(previous_render_command, render_period, is_interactive)
            }
            Mode::Idle => self.update_idle(is_interactive),
        }
    }

    fn update_begin_rendering(&mut self, is_interactive: bool) -> Option<f64> {
        if is_interactive {
            self.mode = Mode::Interactive;
        } else {
            self.mode = Mode::Background;
        }
        Some(self.previous_interactive_render_command)
    }

    fn update_interactive(
        &mut self,
        _: Option<f64>,
        period: Option<f64>,
        is_interactive: bool,
    ) -> Option<f64> {
        if !is_interactive {
            self.mode = Mode::Background;
        }
        let period = period?;
        // Note:  here we use the previous *interactive* command, rather than the
        // general previous command across all modes. This is intentional -- it means
        // that the GUI is responsive when we resume from a period of non-interaction.
        let raw_command = self
            .interactive_policy
            .evaluate(self.previous_interactive_render_command, period);
        self.previous_interactive_render_command = F::clamp_command(raw_command);
        Some(self.previous_interactive_render_command)
    }

    fn update_background(
        &mut self,
        prev_command: Option<f64>,
        period: Option<f64>,
        is_interactive: bool,
    ) -> Option<f64> {
        if is_interactive {
            self.mode = Mode::Interactive;
        }
        let period = period?;
        let prev_command =
            prev_command.expect("ERROR: period data was set, but there is no matching command!");
        let raw_render_command = self.background_policy.evaluate(prev_command, period);
        if raw_render_command <= 0.0 {
            self.mode = Mode::Idle;
        }
        Some(G::clamp_command(raw_render_command))
    }

    fn update_idle(&mut self, is_interactive: bool) -> Option<f64> {
        if is_interactive {
            self.mode = Mode::Interactive;
            Some(self.previous_interactive_render_command)
        } else {
            None
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////

/// The `AdaptiveOptimizationRegulator` is a simple class wrapping a finite state machine
/// that is used to compute the "render quality" (0 = high quality but slow, 1 = low quality
/// but fast), while exploring a fractal interactively with the user.
#[derive(Clone, Debug)]
pub struct AdaptiveOptimizationRegulator {
    render_policy_fsm: FiniteStateMachine<InteractiveFrameRatePolicy, BackgroundFrameRatePolicy>,
    render_start_time: Option<f64>,
    render_period: Option<f64>,
    render_command: Option<f64>,
}

/// For now, keep the regulator simple with some hard-coded policies.
/// Eventually these will be replaced with policies that depend on the
/// measured frame rate data.
impl AdaptiveOptimizationRegulator {
    pub fn new(target_update_period: f64) -> Self {
        Self {
            render_policy_fsm: FiniteStateMachine::new(
                InteractiveFrameRatePolicy::MIN_COMMAND,
                InteractiveFrameRatePolicy::new(target_update_period),
                BackgroundFrameRatePolicy {
                    target_update_period,
                },
            ),
            render_start_time: None,
            render_period: None,
            render_command: None,
        }
    }

    pub fn reset(&mut self) {
        self.render_policy_fsm.reset();
        self.render_start_time = None;
        self.render_period = None;
        self.render_command = None;
    }

    pub fn is_idle(&self) -> bool {
        self.render_policy_fsm.is_idle()
    }

    /// This method is called each time that the `explore` pipeline would like
    /// to render the fractal. It returns an optional value, which, if set,
    /// indicates that the fractal should be rendered, and the floating point
    /// value specifies the render quality value. If unset, in indicates that the
    /// fractal already has been rendered to the screen, and does not need to be
    /// recomputed, allowing the system to save resources.
    pub fn render_required(&mut self, is_interactive: bool) -> Option<f64> {
        self.render_policy_fsm.render_required(
            self.render_command,
            self.render_period,
            is_interactive,
        )
    }

    /// Called by the render pipeline whenever a new render begins.
    /// This is a separate method from `render_required` because we cannot
    /// run two renders at once, and the rendering happens in a separate
    /// background process. This method will be called immediately at the
    /// start of each enw render, and is used to collect accurate timing
    /// data for the finite state machine logic. It caches that data for
    /// use in the `render_required` method.
    pub fn begin_rendering(&mut self, time: f64, command: f64) {
        self.render_start_time = Some(time);
        self.render_period = None;
        self.render_command = Some(command);
    }

    /// Called by the render pipeline whenever the render is completed.
    /// This is the matched method to the `begin_rendering` method, and
    /// is used for accurate data collection on the frame rate. This method
    /// should be called whenever the background thread finishes a render.
    pub fn finish_rendering(&mut self, time: f64) {
        // Note: this method will sometimes be called twice for a single
        // `begin_rendering`, so we add this guard while will only update
        // the period on the first call to `finish_rendering()` after calling
        // `begin_rendering()`.
        if let Some(start_time) = self.render_start_time {
            self.render_period = Some(time - start_time);
            self.render_start_time = None;
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::core::interpolation::{
        InterpolationKeyframe, KeyframeInterpolator, LinearInterpolator,
    };

    use super::*;
    use approx::assert_relative_eq;
    use more_asserts::{assert_ge, assert_le};

    use rand::rngs::StdRng;
    use rand::Rng;
    use rand::SeedableRng;

    #[test]
    fn test_interactive_frame_rate_policy_steady_state_convergence_nominal() {
        // Given the nominal period, the command should not change.
        let target_update_period = 0.025;

        let nominal_period = target_update_period;
        let mut policy: InteractiveFrameRatePolicy =
            InteractiveFrameRatePolicy::new(target_update_period);
        let initial_command = 0.5; // Set an initial non-trivial command for this test.
        for _ in 0..10 {
            let command = policy.evaluate(initial_command, nominal_period);
            assert_relative_eq!(command, initial_command, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_interactive_frame_rate_policy_steady_state_convergence_too_slow() {
        let target_update_period = 0.025;

        let render_period_too_slow = 4.0 * target_update_period;

        // Given a slow period, the command should increase, eventually saturating at 1.0.
        let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
        let mut prev_command = 0.0;
        for _ in 0..25 {
            let next_command = policy.evaluate(prev_command, render_period_too_slow);
            assert_ge!(next_command, prev_command);
            assert_le!(next_command, InteractiveFrameRatePolicy::MAX_COMMAND);
            prev_command = next_command;
        }
        assert_relative_eq!(
            prev_command,
            InteractiveFrameRatePolicy::MAX_COMMAND,
            epsilon = 1e-3
        );
    }

    #[test]
    fn test_interactive_frame_rate_policy_steady_state_convergence_too_fast() {
        let target_update_period = 0.025;

        let render_period_too_fast = 0.2 * target_update_period;

        // Given a slow period, the command should increase, eventually saturating at 1.0.
        let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
        let mut prev_command = 0.05;
        for _ in 0..25 {
            let next_command = policy.evaluate(prev_command, render_period_too_fast);
            assert_le!(next_command, prev_command);
            assert_ge!(next_command, InteractiveFrameRatePolicy::MIN_COMMAND);
            prev_command = next_command;
        }
        assert_relative_eq!(
            prev_command,
            InteractiveFrameRatePolicy::MIN_COMMAND,
            epsilon = 1e-3
        );
    }

    #[test]
    fn test_interactive_frame_rate_policy_bad_input_fuzz_test() {
        let target_update_period = 0.025;

        let periods_to_test = [0.01, 0.4, 1.1, 0.9, 0.99, 1.01, 1.1, 10.0, 100.0];
        let mut rng = StdRng::seed_from_u64(82326745);

        let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
        let mut command = 0.0;
        for _ in 0..500 {
            let index = rng.gen_range(0..periods_to_test.len());
            let period = target_update_period * periods_to_test[index];
            command = policy.evaluate(command, period);
            assert_le!(command, InteractiveFrameRatePolicy::MAX_COMMAND);
            assert_ge!(command, InteractiveFrameRatePolicy::MIN_COMMAND);
        }
    }

    /// Model that maps from a command to a period, emulating the I/O behavior of the full
    /// render pipeline. The "fast" variant will emulate a system that always renders faster
    /// than the target period, regardless of the command. It does however satisfy the requirement
    /// that a higher command will always yield a shorter period.
    fn build_fast_render_proxy_model(
        target_period: f64,
    ) -> KeyframeInterpolator<f64, f64, LinearInterpolator> {
        let keyframes: Vec<InterpolationKeyframe<f64, f64>> = vec![
            InterpolationKeyframe {
                query: 0.0,
                value: 0.95 * target_period,
            },
            InterpolationKeyframe {
                query: 0.1,
                value: 0.7 * target_period,
            },
            InterpolationKeyframe {
                query: 0.4,
                value: 0.4 * target_period,
            },
            InterpolationKeyframe {
                query: 1.0,
                value: 0.05 * target_period,
            },
        ];
        KeyframeInterpolator::new(keyframes, LinearInterpolator)
    }

    // Same as above, but for a proxy that will never hit the target period.
    fn build_slow_render_proxy_model(
        target_period: f64,
    ) -> KeyframeInterpolator<f64, f64, LinearInterpolator> {
        let keyframes: Vec<InterpolationKeyframe<f64, f64>> = vec![
            InterpolationKeyframe {
                query: 0.0,
                value: 10.0 * target_period,
            },
            InterpolationKeyframe {
                query: 0.1,
                value: 5.0 * target_period,
            },
            InterpolationKeyframe {
                query: 0.4,
                value: 2.0 * target_period,
            },
            InterpolationKeyframe {
                query: 1.0,
                value: 1.05 * target_period,
            },
        ];
        KeyframeInterpolator::new(keyframes, LinearInterpolator)
    }

    // Same as above, but for a proxy that will hit the target at an intermediate command.
    fn build_nominal_render_proxy_model(
        target_period: f64,
    ) -> KeyframeInterpolator<f64, f64, LinearInterpolator> {
        let keyframes: Vec<InterpolationKeyframe<f64, f64>> = vec![
            InterpolationKeyframe {
                query: 0.0,
                value: 2.0 * target_period,
            },
            InterpolationKeyframe {
                query: 0.1,
                value: 1.3 * target_period,
            },
            InterpolationKeyframe {
                query: 0.321,
                value: target_period,
            },
            InterpolationKeyframe {
                query: 0.5,
                value: 0.7 * target_period,
            },
            InterpolationKeyframe {
                query: 1.0,
                value: 0.06 * target_period,
            },
        ];
        KeyframeInterpolator::new(keyframes, LinearInterpolator)
    }

    /// Simulates a combined "controller and render pipeline proxy", allowing us to
    /// test the closed-loop performance in various scenarios. It monitors convergence
    /// of the system toward the expected steadty state behavior, and returns true if the system
    /// converged within the specified number of iterations.
    fn simulate_controller(
        policy: &mut InteractiveFrameRatePolicy,
        initial_command: f64,
        render_proxy: &KeyframeInterpolator<f64, f64, LinearInterpolator>,
        num_iterations: usize,
        steady_state_period: f64,
        steady_state_command: f64,
        convergence_tol: f64,
    ) -> bool {
        let mut prev_command = initial_command;
        for _ in 0..num_iterations {
            let prev_period = render_proxy.evaluate(prev_command);
            let next_command = policy.evaluate(prev_command, prev_period);
            let next_period = render_proxy.evaluate(next_command);

            let prev_cmd_err = (prev_command - steady_state_command).abs();
            let next_cmd_err = (next_command - steady_state_command).abs();
            assert_le!(next_cmd_err, prev_cmd_err);

            let prev_period_err = (prev_period - steady_state_period).abs();
            let next_period_err = (next_period - steady_state_period).abs();
            assert_le!(next_period_err, prev_period_err);

            prev_command = next_command;

            // If the command and period errors are both small enough, we can consider the system
            // to have converged to a steady state.
            if next_cmd_err < convergence_tol && next_period_err < convergence_tol {
                return true;
            }
        }
        // If we reach here, the system did not converge within the specified iterations.
        false
    }

    #[test]
    fn test_interactive_frame_rate_policy_closed_loop_fast_render() {
        let target_update_period = 0.025;
        let render_proxy = build_fast_render_proxy_model(target_update_period);
        let steady_state_period = *render_proxy.values().first().unwrap();
        let steady_state_command = InteractiveFrameRatePolicy::MIN_COMMAND;

        let initial_commands = [0.0, 0.3, 0.7, 1.0];
        let max_iterations = 10;
        for initial_command in initial_commands {
            let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
            let converged = simulate_controller(
                &mut policy,
                initial_command,
                &render_proxy,
                max_iterations,
                steady_state_period,
                steady_state_command,
                1e-2,
            );
            assert!(
                converged,
                "Failed to converge with initial command: {:.6}",
                initial_command
            );
        }
    }

    #[test]
    fn test_interactive_frame_rate_policy_closed_loop_slow_render() {
        let target_update_period = 0.025;
        let render_proxy = build_slow_render_proxy_model(target_update_period);
        let steady_state_period = *render_proxy.values().last().unwrap();
        let steady_state_command = InteractiveFrameRatePolicy::MAX_COMMAND;

        let initial_commands = [0.0, 0.3, 0.7, 1.0];
        let max_iterations = 25;
        for initial_command in initial_commands {
            let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
            let converged = simulate_controller(
                &mut policy,
                initial_command,
                &render_proxy,
                max_iterations,
                steady_state_period,
                steady_state_command,
                1e-2,
            );
            assert!(
                converged,
                "Failed to converge with initial command: {:.6}",
                initial_command
            );
        }
    }

    #[test]
    fn test_interactive_frame_rate_policy_closed_loop_nominal_render() {
        let target_update_period = 0.025;
        let render_proxy = build_nominal_render_proxy_model(target_update_period);
        let target_index = 2;
        let steady_state_period = render_proxy.values()[target_index];
        let steady_state_command = render_proxy.queries()[target_index];

        let initial_commands = [0.0, 0.3, 0.7, 1.0];
        let max_iterations = 25;
        for initial_command in initial_commands {
            let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
            let converged = simulate_controller(
                &mut policy,
                initial_command,
                &render_proxy,
                max_iterations,
                steady_state_period,
                steady_state_command,
                1e-3,
            );
            assert!(
                converged,
                "Failed to converge with initial command: {:.6}",
                initial_command
            );
        }
    }
}
