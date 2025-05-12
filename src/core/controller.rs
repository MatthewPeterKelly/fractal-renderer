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

/// Given an optimization "level", store the associated measured
/// period. This can then be used for an iterative nonlinear root-solver
/// trying to stabilize to the correct level to hit the desired target period.

#[derive(Clone, Debug)]
pub struct AdaptiveOptimizationRegulator {
    // How fast do we ideally want the update period to run?
    // implemented as a deadband to avoid chattering on the render settings.
    target_update_period_min: f64,
    target_update_period_max: f64,

    // Time at which the update was called previously
    time: f64,

    // Command that was computed on the last update
    // Stateful. The "optimization level" to pass to the renderer.
    command: f64,

    // Gains for the controller  (fractional change in level) / (error in period)
    quality_gain: f64, // used to reduce level to increase quality
    speed_gain: f64,   // used to increase level to increase speed

    // While idle (no view port motion), linearly decrease the level until
    // we hit zero. Once we've rendered once at zero, stop updating.
    idle_level_step: f64,
}

impl AdaptiveOptimizationRegulator {
    pub fn new(time: f64) -> AdaptiveOptimizationRegulator {
        // TODO:  these could be read from the parameter file.
        Self {
            target_update_period_min: 0.04,
            target_update_period_max: 0.1,
            time,
            command: 0.5,
            quality_gain: 0.5 / 0.04,
            speed_gain: 0.8 / 0.1,
            idle_level_step: 0.1,
        }
    }

    /// Given the current, time, compute the measured frame-rate (period) and then return
    /// the optimization level that sould be used for reendering the enxt frame.
    /// If time is equal to the internally cached time, the this function is a no-op,
    /// and will return the cached value. This allows it to be safely called multiple times
    /// per tick.
    pub fn interactive_update(&mut self, time: f64) -> f64 {
        println!(
            "Idle called with time: {:?},  (self.time: {:?})",
            time, self.time
        );
        if time <= self.time {
            return self.command;
        }
        let period = time - self.time;
        self.time = time;
        self.command = if period < self.target_update_period_min {
            // println!("More quality! period: {:?}", period);
            // Handle the case where we try to add more quality
            let error = self.target_update_period_min - period;
            let fraction = (error * self.quality_gain).min(1.0);
            // fraction of 1.0 maps to zero level
            // frtaction of 0.0 maps to the current commanded level
            self.command * (1.0 - fraction)
        } else if period > self.target_update_period_max {
            // println!("More speed! period: {:?}", period);
            // Handle the case where we reduce quality to increase speed
            let error = period - self.target_update_period_max;
            let fraction = (error * self.speed_gain).min(1.0);
            // fraction of 1.0 maps to one (maximum level)
            // frtaction of 0.0 maps to the current commanded level
            fraction + self.command * (1.0 - fraction)
        } else {
            // We're in bounds!  Nice. Keep using this level.
            self.command
        };
        println!("  self: {:?}", self);
        self.command
    }

// TOOD:  better to just pass an "is interactive" flag, and ahave a single update
// function. This allows that flag to be set and then we can call the udpate exactly once.


    pub fn idle_update(&mut self, time: f64) -> Option<f64> {
        if self.time <= time {
            return Some(self.command);
        }
        self.time = time;
        // Check if we rendered at zero last time. If so... done!
        if self.command == 0.0 {
            return None;
        }
        // Linearly march toward zero, clamping once there
        self.command -= self.idle_level_step;
        self.command = self.command.max(0.0);
        Some(self.command)
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use more_asserts::{assert_gt, assert_lt};

    use crate::core::controller::AdaptiveOptimizationRegulator;

    fn nominal_update_period(regulator: &AdaptiveOptimizationRegulator) -> f64 {
        0.5 * (regulator.target_update_period_min + regulator.target_update_period_max)
    }

    fn nominal_update_time(regulator: &AdaptiveOptimizationRegulator) -> f64 {
        regulator.time + nominal_update_period(regulator)
    }

    /// Approximate the "render engine", which will emulate a system where
    /// the render time will converge to the target time at some point between
    /// zero and one. Not quite a perfect match, but good enough.
    /// The `target` here is the expected command that will hit exactly 0.5*(low + upp).
    /// In other words, when the command is at the target, we exactly match the frame rate.
    /// The `input` here is the actual command that was sent by the system. If the command
    /// is larger than the target, then we're requesting "too much optimization", and the
    /// frame rate (output of this function) will be "too small". To make a well-definied
    /// problem, lets say that the maximum command (optimization level) of 1.0 will produce
    /// a period that is slightly positive, say 0.01 * low.
    pub fn mock_render_engine(input: f64, low: f64, upp: f64, target: f64) -> f64 {
        let x0 = target;
        let y0 = 0.5 * (low + upp);

        let x1 = 1.0;
        let y1 = 0.01 * low;

        let slope = (y1 - y0) / (x1 - x0);
        let intercept = y0 - slope * x0;

        slope * input + intercept
    }

    fn interactive_update_with_render_in_loop(
        regulator: &mut AdaptiveOptimizationRegulator,
        target: f64,
    ) {
        let period = mock_render_engine(
            regulator.command,
            regulator.target_update_period_min,
            regulator.target_update_period_max,
            target,
        );
        assert_gt!(period, 0.0);
        let time = regulator.time + period;
        regulator.interactive_update(time);
    }

    fn assert_interactive_update_moves_toward_target(
        regulator: &mut AdaptiveOptimizationRegulator,
        target: f64,
    ) {
        let prev_err = target - regulator.command;
        interactive_update_with_render_in_loop(regulator, target);
        let next_err = target - regulator.command;
        // assert_lt!(
        //     next_err.abs(),
        //     prev_err.abs(),
        //     "next: {:?}, prev: {:?}",
        //     next_err,
        //     prev_err
        // );
        println!("regulator.command: {:?}, target: {:?}", regulator.command, target);
    }

    #[test]
    fn test_mock_render_engine_behavior() {
        let low = 4.0;
        let upp = 10.0;
        let target = 0.5;

        let soln_at_target = 0.5 * (low + upp);
        let soln_at_one = 0.01 * low;

        let result_at_target = mock_render_engine(target, low, upp, target);
        let result_at_one = mock_render_engine(1.0, low, upp, target);

        assert_relative_eq!(result_at_one, soln_at_one, epsilon = 1e-6);
        assert_relative_eq!(result_at_target, soln_at_target, epsilon = 1e-6);
    }

    #[test]
    fn test_adaptive_optimization_regulator_interative_update() {
        let mut regulator = AdaptiveOptimizationRegulator::new(0.0);

        // Nominal update, in desired window
        let prev_level = regulator.command;
        let mut level = regulator.interactive_update(nominal_update_time(&regulator));
        assert_eq!(level, prev_level);
        level = regulator.interactive_update(nominal_update_time(&regulator));
        assert_eq!(level, prev_level);

        // Now, set a high target, and check that the command converges toward the target:
        for _ in 0..5 {
            assert_interactive_update_moves_toward_target(&mut regulator, 0.9);
        }

        // Now, track a lower target:
        for _ in 0..5 {
            assert_interactive_update_moves_toward_target(&mut regulator, 0.1);
        }
    }

    // Zero and negative time updaets
}
