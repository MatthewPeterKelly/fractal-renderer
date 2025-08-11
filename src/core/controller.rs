use more_asserts::{assert_ge, assert_gt};

use crate::core::interpolation::{InterpolationKeyframe, KeyframeInterpolator, LinearInterpolator};

#[derive(Clone, Debug)]
pub enum Target {
    Position { pos_ref: f64, max_vel: f64 },
    Velocity { vel_ref: f64 },
}

#[derive(Clone, Debug)]
pub struct PointTracker {
    position: f64,
    target: Target,
    time: f64,
}

impl PointTracker {
    pub fn new(time: f64, pos: f64) -> PointTracker {
        PointTracker {
            position: pos,
            target: Target::Velocity { vel_ref: 0.0 },
            time,
        }
    }

    // Indicates the controller should drop an active velocity command
    // but keep tracking a position target until it is reached.
    pub fn set_idle_target(&mut self) {
        if let Target::Velocity { vel_ref: _ } = self.target {
            self.target = Target::Velocity { vel_ref: 0.0 };
        }
    }

    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    /// Sets the position and clears any actively tracked target.
    pub fn set_position(&mut self, position: f64) {
        self.position = position;
        self.target = Target::Velocity { vel_ref: 0.0 };
    }

    pub fn update_and_return_pos(&mut self, time: f64) -> f64 {
        let delta_time = time - self.time;
        self.time += delta_time;
        self.update_position(delta_time);
        self.position
    }

    fn update_position(&mut self, delta_time: f64) {
        match self.target {
            Target::Position { pos_ref, max_vel } => {
                let pos_err = pos_ref - self.position;
                let max_pos_delta = (max_vel * delta_time).abs();

                if pos_err.abs() < max_pos_delta {
                    // We reached the target!
                    self.position = pos_ref;
                    self.target = Target::Velocity { vel_ref: 0.0 };
                } else {
                    // Move toward the target at constant max velocity:
                    let pos_err_clamped = pos_err.clamp(-max_pos_delta, max_pos_delta);
                    self.position += pos_err_clamped;
                }
            }
            Target::Velocity { vel_ref } => {
                self.position += vel_ref * delta_time;
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////

// The frame rate policy is a mapping from the measured period to the
// command that should be used on the next render, conditioned on the
// command that was used to generate the current measured period.
//
// This can be thought of as a root-finding exercise, where the the
// objective is to find the command that will yield the desired period.
// In this case, the "function" that we're iterating over is the entire
// render pipeline.
//
// One way to do this root-solve is with quassi-Newton method, which requires
// that we be able to locally approximate the function with a linear model.
// Directly computing the derivative of the function is not possible, because
// the render pipeline is not consistent across frames -- the CPU load might
//
// change, and also the fractal being rendered might change, so we've got to
// work with a single evaluation.
//
// There is another tricky detail -- we need to ensure that the command is bounded
// to the domain of [0,1], which is not typically part of a quassi newton method.
//
// This problem has some nice features that we can exploit. The first is that we
// know the render pipeline is monotonic -- increasing the command will always
// reduce the period, until it saturates (given a fixed CPU and render task).
// Additionally, the command is
// bounded to [0,1], and the period is bounded to [0, max_expected_period].
//
// We can use this information to construct a piecewise linear model of the function
// that is always invertible, and which is guaranteed to have a single unique solution
// for any period in the range [0, max_expected_period]. The model is constructed
// by interpolation between three keyframes, where the first and last keyframes are
// specified by the problem itself, and the middle keyframe is updated each time based
// on the measured period and the command that produced it. As a result, this model is
// most accurate near the current measurement, and the two boundary keyframes primarily
// serve to provide a reasonable guess at the local deriviatives.


// This model approximates the mapping from command to period as:
// period(command) = scale * exp(-command)
/// where scale is a positive constants.
struct ExponentialRenderPeriodModel {
    scale: f64,
}


impl ExponentialRenderPeriodModel {

        /// Fits a new exponential model to a single point
    pub fn new(command: f64, period: f64, ) -> Self {

   assert_ge!(command, 0.0, "Sampled command must be positive!");
   assert_gt!(period, 0.0, "Sampled period must be positive!");
    let scale = period / (-command).exp();
    Self { scale }
    }

    pub fn compute_period_from_command(&self, command: f64) -> f64 {
        self.scale * (-command).exp()
    }

    pub fn compute_command_from_period(&self, period: f64) -> f64 {
        assert_gt!(period, 0.0, "Period must be positive!");
        - (period / self.scale).ln()
    }

}


#[derive(Clone, Debug)]
pub struct InteractiveFrameRatePolicy {
    // User-specified parameters for the timing policy, which do not change after construction.
    target_update_period: f64,

    // The command is cached here for use on the next evaluation to update the model.
    command: f64, // The command that was produced by the last evaluation.
}

impl InteractiveFrameRatePolicy {
    pub const MAX_COMMAND: f64 = 1.0;
    pub const MIN_COMMAND: f64 = 0.0;
    pub const MIN_PERIOD: f64 = 0.0;

    pub fn new( target_update_period: f64) -> InteractiveFrameRatePolicy {
        InteractiveFrameRatePolicy {
            target_update_period,
            command:Self::MIN_COMMAND,
        }
    }

    pub fn evaluate_policy(&mut self, measured_period: f64) -> f64 {

        let prev_cmd = self.command;  // HACK!!! Save the previous command for debugging.

        // (0) Ensure that the measurement is within the expected range, which in turn
        //     will ensure that the model remains invertable (and we can update the policy).
        let clamped_measured_period =
            measured_period.max(Self::MIN_PERIOD);

        // (1) Construct a simple exponential model of the system, based on measured period.
            let model = ExponentialRenderPeriodModel::new(self.command, clamped_measured_period);

        // (2) Evaluate the model at the target update period.
        let raw_command = model.compute_command_from_period(self.target_update_period);

        // (3) The policy will sometimes produce out-of-bounds commands. This is intentional, and
        //     is what allows the model to correctly handle saturation in the cases where there is
        //     no solution (all values of the command result in a period that is either above or below the target).
        let command_mid = raw_command.clamp(Self::MIN_COMMAND, Self::MAX_COMMAND);

        self.command = command_mid.clamp(prev_cmd - 0.1, prev_cmd + 0.1);

        println!(
            "Evaluating policy: measured_period = {:.6}, prev command = {:.6};  Result: {:.6} ->  {:.6} -> {:.6}",
            measured_period, prev_cmd, raw_command, command_mid, self.command
        ); // HACK!!! Remove this in production code.

        self.command
    }
}



///////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct AdaptiveOptimizationRegulator {}

impl AdaptiveOptimizationRegulator {
    pub fn new(_time: f64) -> Self {
        Self {}
    }

    pub fn update(&mut self, _period: f64, _user_interaction: bool) -> Option<f64> {
        None
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use approx::assert_relative_eq;
    use more_asserts::{assert_ge, assert_le};

    use rand::rngs::StdRng;
    use rand::Rng;
    use rand::SeedableRng;


    #[test]
    fn test_exponential_render_period_model() {
        {
            let command = 0.0;
            let period = 0.1;
            let model = ExponentialRenderPeriodModel::new(command, period);
            assert_relative_eq!(model.compute_period_from_command(command), period, epsilon = 1e-6);
        }
          {
            let command = 1.0;
            let period = 0.01;
            let model = ExponentialRenderPeriodModel::new(command, period);
            assert_relative_eq!(model.compute_period_from_command(command), period, epsilon = 1e-6);
        }

    }

    #[test]
    fn test_interactive_frame_rate_policy_steady_state_convergence_nominal() {
        // Given the nominal period, the command should not change.
        let target_update_period = 0.025;
        let nominal_period = target_update_period;
        let mut policy: InteractiveFrameRatePolicy = InteractiveFrameRatePolicy::new(target_update_period);
        policy.command = 0.5; // Set an initial non-trivial command for this test.
        let initial_command = policy.command;
        for _ in 0..10 {
            let command = policy.evaluate_policy(nominal_period);
            assert_relative_eq!(command, initial_command, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_interactive_frame_rate_policy_steady_state_convergence_too_slow() {
        let target_update_period = 0.025;
        let render_period_too_slow = 4.0 * target_update_period;

        // Given a slow period, the command should increase, eventually saturating at 1.0.
        let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
        for _ in 0..25 {
            let prev_command = policy.command;
            let next_command = policy.evaluate_policy(render_period_too_slow);
            assert_ge!(next_command, prev_command);
            assert_le!(next_command, InteractiveFrameRatePolicy::MAX_COMMAND);
        }
        assert_relative_eq!(
            policy.command,
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
        policy.command = 0.05; // Set an initial non-trivial command to check convergence.
        for _ in 0..25 {
            let prev_command = policy.command;
            let next_command = policy.evaluate_policy(render_period_too_fast);
            assert_le!(next_command, prev_command);
            assert_ge!(next_command, InteractiveFrameRatePolicy::MIN_COMMAND);
        }
        assert_relative_eq!(
            policy.command,
            InteractiveFrameRatePolicy::MIN_COMMAND,
            epsilon = 1e-3
        );
    }

    #[test]
    fn test_interactive_frame_rate_policy_bad_input_fuzz_test() {
        let target_update_period = 0.025;

        let periods_to_test = [
            0.01 ,
            0.4 ,
            1.1 ,
            0.9 ,
            0.99 ,
            1.01 ,
            1.1 ,
            10.0 ,
            100.0 ,
        ];
        let mut rng = StdRng::seed_from_u64(82326745);

        let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
        for _ in 0..500 {
            let index = rng.gen_range(0..periods_to_test.len());
            let period = target_update_period * periods_to_test[index];
            let command = policy.evaluate_policy(period);
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
                value: 0.7* target_period,
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
        render_proxy: &KeyframeInterpolator<f64, f64, LinearInterpolator>,
        num_iterations: usize,
        steady_state_period: f64,
        steady_state_command: f64,
        convergence_tol: f64,
    ) -> bool {
        for _ in 0..num_iterations {
            let prev_command = policy.command;
            let prev_period = render_proxy.evaluate(prev_command);
            let next_command = policy.evaluate_policy(prev_period);
            let next_period = render_proxy.evaluate(next_command);

            // println!(
            //     "Prev Command: {:.6}, Prev Period: {:.6}, Next Command: {:.6}, Next Period: {:.6}",
            //     prev_command, prev_period, next_command, next_period
            // );  // HACK!!!

            let prev_cmd_err = (prev_command - steady_state_command).abs();
            let next_cmd_err = (next_command - steady_state_command).abs();
            assert_le!(next_cmd_err, prev_cmd_err);

            let prev_period_err = (prev_period - steady_state_period).abs();
            let next_period_err = (next_period - steady_state_period).abs();
            assert_le!(next_period_err, prev_period_err);

            println!(
                "Prev Command Error: {:.6}, Next Command Error: {:.6}, Prev Period Error: {:.6}, Next Period Error: {:.6}",
                prev_cmd_err, next_cmd_err, prev_period_err, next_period_err
            );  // HACK!!

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

        let initial_commands = [0.0,0.3,0.7,1.0];
        let max_iterations = 10;
        for initial_command in initial_commands {
            let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
            policy.command = initial_command;
            let converged = simulate_controller(
                &mut policy,
                &render_proxy,max_iterations,
                steady_state_period,
                steady_state_command,
                1e-2,
            );
            assert!(
                converged, "Failed to converge with initial command: {:.6}", initial_command
            );
        }
    }

        #[test]
    fn test_interactive_frame_rate_policy_closed_loop_slow_render() {
        let target_update_period = 0.025;
        let render_proxy = build_slow_render_proxy_model(target_update_period);
        let steady_state_period = *render_proxy.values().last().unwrap();
        let steady_state_command = InteractiveFrameRatePolicy::MAX_COMMAND;

        let initial_commands = [0.0,0.3,0.7,1.0];
        let max_iterations = 10;
        for initial_command in initial_commands {
            let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
            policy.command = initial_command;
            let converged = simulate_controller(
                &mut policy,
                &render_proxy,max_iterations,
                steady_state_period,
                steady_state_command,
                1e-2,
            );
            assert!(
                converged, "Failed to converge with initial command: {:.6}", initial_command
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

        let initial_commands = [0.0,0.3,0.7,1.0];
        let max_iterations = 15;
        for initial_command in initial_commands {
            let mut policy = InteractiveFrameRatePolicy::new(target_update_period);
            policy.command = initial_command;
            let converged = simulate_controller(
                &mut policy,
                &render_proxy,max_iterations,
                steady_state_period,
                steady_state_command,
                1e-3,
            );
            assert!(
                converged, "Failed to converge with initial command: {:.6}", initial_command
            );
        println!("==========================================")
        }

    }
}
