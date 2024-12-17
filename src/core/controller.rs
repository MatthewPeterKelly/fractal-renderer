#[cfg(test)]
use crate::core::dynamical_systems::SimpleLinearControl;

#[derive(Debug, Clone, Copy)]
#[cfg(test)]
pub struct PointState {
    pub pos: f64,
    pub vel: f64,
}

#[cfg(test)]
impl PointState {
    pub fn time_step_physics(&mut self, delta_time: f64, acc: f64) {
        // Symplectic Euler's method... stable first order ODE step.
        self.vel += acc * delta_time;
        self.pos += self.vel * delta_time;
    }
}

#[derive(Clone, Debug)]
#[cfg(test)]
pub enum Target {
    Position { pos_ref: f64, max_vel: f64 },
    Velocity { vel_ref: f64 },
}

#[derive(Debug, Clone, Copy)]
#[cfg(test)]

pub struct Controller {
    pub gains: PointState,
    pub vel_err_int: f64, // velocity error integrator state
}

#[cfg(test)]
impl Controller {
    #[cfg(test)]
    pub fn gains_from_damping_and_natural_frequency(
        damping_ratio: f64,
        natural_frequency: f64,
    ) -> PointState {
        let kd = 2.0 * damping_ratio * natural_frequency;
        let kp = natural_frequency * natural_frequency;
        PointState { pos: kp, vel: kd }
    }

    #[cfg(test)]
    pub fn from_rise_time(rise_time: f64) -> Controller {
        // See the `test_closed_loop_controller_critically_damped_rise_time()`
        // test in `ode_solvers` for an idea of how the 3.357... value is found.
        Controller::new(Self::gains_from_damping_and_natural_frequency(
            1.0,
            SimpleLinearControl::CRITICALLY_DAMPED_RISE_TIME_SCALE_FACTOR / rise_time,
        ))
    }

    #[cfg(test)]
    pub fn new(gains: PointState) -> Controller {
        Controller {
            gains,
            vel_err_int: 0.0,
        }
    }

    /// Fancy PD controller that works for both "position" and "velocity"
    /// control modes. The key feature here is that we can smoothly switch
    /// between the two modes while supporting velocity limits in position
    /// control. This allows the controller to accept position references
    /// that are arbitrarily far away, by smoothly ramping up to the max
    /// speed and then ramping back down to zero as it reaches the target.
    /// Note: the "integral" gain on the velocity controller does not have
    /// a proper "anti-wind-up" feature, because we assume that the command
    /// acceleration will be applied perfectly to the state by PointTracker.
    #[cfg(test)]
    pub fn update_and_compute_acceleration(
        &mut self,
        state: &PointState,
        delta_time: f64,
        target: &Target,
    ) -> f64 {
        let (pos_err, vel_err) = match *target {
            Target::Position { pos_ref, max_vel } => {
                // Fancy math: compute the max pos err S.T. velocity saturates at max_vel
                // Note:  should be positive; all three terms on the RHS are positive...
                // but if the user does something silly, then the `abs()` prevents a crash.
                let max_pos_err = (self.gains.vel * max_vel / self.gains.pos).abs();

                let pos_err = pos_ref - state.pos;
                let pos_err = pos_err.clamp(-max_pos_err, max_pos_err);
                self.vel_err_int = 0.0;
                (pos_err, -state.vel)
            }
            Target::Velocity { vel_ref } => {
                let vel_err = vel_ref - state.vel;
                self.vel_err_int += delta_time * vel_err;
                (self.vel_err_int, vel_err)
            }
        };

        // Simple PD controller to compute the acceleration
        pos_err * self.gains.pos + vel_err * self.gains.vel
    }
}

#[derive(Clone, Debug)]
#[cfg(test)]
pub struct PointTracker {
    controller: Controller,
    state: PointState,
    target: Target,
    time: f64,
    max_time_step: f64,
}

#[cfg(test)]
impl PointTracker {
    pub const MINIMUM_INTEGRATION_STEP_COUNT_PER_RISE_TIME: f64 = 5.0;

    pub fn new(time: f64, pos: f64, rise_time: f64) -> PointTracker {
        PointTracker {
            controller: Controller::from_rise_time(rise_time),
            state: PointState { pos, vel: 0.0 },
            target: Target::Velocity { vel_ref: 0.0 },
            time,
            max_time_step: rise_time / Self::MINIMUM_INTEGRATION_STEP_COUNT_PER_RISE_TIME,
        }
    }

    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    /// Updates the simulation, potentially running several update steps,
    /// bringing the state up to the current time.
    pub fn update_and_return_pos(&mut self, time: f64) -> f64 {
        let delta_time = time - self.time;
        let num_steps = (delta_time / self.max_time_step).ceil();
        let dt = delta_time / num_steps;

        for _ in 0..(num_steps as u32) {
            self.physics_step(dt);
        }
        self.state.pos
    }

    fn physics_step(&mut self, delta_time: f64) {
        let acc =
            self.controller
                .update_and_compute_acceleration(&self.state, delta_time, &self.target);
        self.state.time_step_physics(delta_time, acc);
        self.time += delta_time;
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use more_asserts::assert_le;

    use super::*;

    #[test]
    fn test_steady_state_at_position_target() {
        let rise_time = 0.2;
        let pos_init = 0.5;
        let time_init = 1.1;
        let mut tracker = PointTracker::new(time_init, pos_init, rise_time);

        // Select a velocity limit that we will actually hit...
        // ... but then run long enough to actually hit the target.

        let pos_ref = -0.4;
        let max_vel = 8.0;
        tracker.set_target(Target::Position { pos_ref, max_vel });

        // Check the the "internal physics sub-step" works properly:
        let time_final = time_init + 20.0 * rise_time;
        let pos_final = tracker.update_and_return_pos(time_final);

        let tol = 1e-3;
        assert_relative_eq!(tracker.time, time_final, epsilon = tol);
        assert_eq!(tracker.state.pos, pos_final);
        assert_relative_eq!(pos_final, pos_ref, epsilon = tol);
        assert_relative_eq!(tracker.state.vel, 0.0, epsilon = tol);
    }

    #[test]
    fn test_position_mode_velocity_limit() {
        let rise_time = 0.2;
        let pos_init = 0.5;
        let time_init = 1.1;
        let mut tracker = PointTracker::new(time_init, pos_init, rise_time);

        // Small velocity limit, far away position --> ensure we don't violate the limits
        let max_vel = 0.2;
        let pos_ref = -25.0; // Don't actually expect to reach this
        tracker.set_target(Target::Position { pos_ref, max_vel });
        let dt = 0.3 * rise_time;
        let time_final = tracker.time + 3.0 * rise_time;
        while tracker.time < time_final {
            tracker.update_and_return_pos(tracker.time + dt);
            assert_le!(tracker.state.vel.abs(), max_vel);
        }
        // Verify that we actually hit steady-state at the max velocity
        assert_relative_eq!(tracker.state.vel, -max_vel, epsilon = 1e-5);

        // Switch directions, and check velocity limits again.
        let pos_ref = 51.0;
        tracker.set_target(Target::Position { pos_ref, max_vel });
        let time_final = tracker.time + 3.0 * rise_time;
        while tracker.time < time_final {
            tracker.update_and_return_pos(tracker.time + dt);
            assert_le!(tracker.state.vel.abs(), max_vel);
        }
        // Verify that we actually hit steady-state at the max velocity
        assert_relative_eq!(tracker.state.vel, max_vel, epsilon = 1e-5);
    }

    #[test]
    fn test_position_mode_convergence() {
        let rise_time = 0.2;
        let pos_init = 1.0;
        let time_init = 1.1;
        let mut tracker = PointTracker::new(time_init, pos_init, rise_time);

        // Large velocity limit, close position --> verify critically damped convergence
        let max_vel = 1000.0;
        let pos_ref = 2.0;
        tracker.set_target(Target::Position { pos_ref, max_vel });
        let dt = 0.1 * rise_time;
        let time_final = time_init + 8.0 * rise_time;
        while tracker.time < time_final {
            let time = tracker.time + dt;
            let cached_state = tracker.state;
            tracker.update_and_return_pos(time);
            let old_pos_err = (cached_state.pos - pos_ref).abs();
            let new_pos_err = (tracker.state.pos - pos_ref).abs();
            assert_le!(new_pos_err, old_pos_err);
            assert_le!(tracker.state.vel, max_vel);
        }
    }

    #[test]
    fn test_velocity_mode_convergence() {
        let rise_time = 0.4;
        let pos_init = -0.3;
        let time_init = 1.1;
        let mut tracker = PointTracker::new(time_init, pos_init, rise_time);

        let vel_ref = 0.9;
        let time_final = time_init + 3.0 * rise_time;

        tracker.set_target(Target::Velocity { vel_ref });

        let dt = 0.3 * rise_time;
        let overshoot_tol = 0.1;
        while tracker.time < time_final {
            let time = tracker.time + dt;
            let cached_state = tracker.state;
            tracker.update_and_return_pos(time);
            let old_vel_err = (cached_state.vel - vel_ref).abs();
            let new_vel_err = (tracker.state.vel - vel_ref).abs();
            assert_le!(new_vel_err, old_vel_err + overshoot_tol);
        }
    }
}
