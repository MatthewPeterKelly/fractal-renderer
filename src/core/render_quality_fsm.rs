//! Simple FSM (finite state machine) that is used to regulate the
//! render quality command for the render pipeline. While the user
//! is actively interacting with the system, we want to hit a target
//! frame rate, even if the render quality is low. However, once the
//! user stops interacting, then we need to quickly crank up the render
//! quality regardless of frame rate. Finally, once we've rendered at
//! high quality, we should shut down the render pipeline to conserve
//! resources (no need to spin at max CPU while idle...).


// (1) updat the Interactive state to go back to the optional f64 for time. I realized that we always need the previous command.
// (2) pass the previous command to the user policy (making it stateless)
// (3) use the same generic interface for the "idle policy" (allow the user to pass in an idle and a interactive policy).
// (4) make max delta an implementation detail of the user policies
// (5) use the same interface and design pattern for both interactive and background modes.


pub trait RenderQualityPolicy {
    /// @param previous_command: last render command that was completed
    /// @param measured_period: how long did that render command take to complete?
    /// @return: render quality command (0 = maximum quality; 1 = maximum speed)
    ///     or None (indicating that the render pipeline should not run).
    fn evaluate(&mut self,  previous_command: f64, measured_period: f64) -> Option<f64>;
}

use more_asserts::{assert_ge, assert_gt, assert_le};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    BeginRendering,
    Interactive,
    Background,
    Idle,
}


#[derive(Debug)]
pub struct FiniteStateMachine<F,G>
where
    F: RenderQualityPolicy,
    G: RenderQualityPolicy,
{
    mode: Mode,  // which mode are we in right now?
    begin_rendering_command: f64,  // what is the command to send when we first start rendering?
    prev_render_command: f64,
    prev_render_time: f64,
    interactive_policy: F,
    background_policy: G,
}

impl<F,G> FiniteStateMachine<F, G>
where
    F: RenderQualityPolicy,
    G: RenderQualityPolicy,
{
    /// Create a new FSM for regularing the render quality.
    pub fn new(initial_command: f64,interactive_policy: F, background_policy: G) -> Self {
        assert_ge!(initial_command, 0.0);
        assert_le!(initial_command, 1.0);
        let initial_command = initial_command.clamp(0.0, 1.0);
        Self {
            mode: Mode::BeginRendering,
            begin_rendering_command: initial_command,
            prev_render_command: initial_command,
            prev_render_time: 0.0,
            interactive_policy,
            background_policy,
        }
    }

    fn transition_logic(&mut self, is_interactive: bool) {
        match (self.mode, is_interactive) {
            (Mode::BeginRendering, true) => {
                self.mode = Mode::Interactive;
            }
                     (Mode::BeginRendering, false) => {
                self.mode = Mode::Background;
                // TODO?
            }
            (Mode::Background, true) => {
                self.mode = Mode::Interactive;
            }
            (Mode::Interactive, false) => {
                self.interactive.reset(self.prev_command);
                self.mode = Mode::Background;
            }
            (Mode::Idle, true) => {
                self.mode = Mode::Interactive;
            }
            // Otherwise, remain in current mode
            _ => { /* no-op */ }
        }
    }

    /// Update the render FSM and return the render command, if any is needed.
    pub fn update(&mut self, time: f64, is_interactive: bool) -> Option<f64> {
        self.transition_logic(is_interactive);

        // --- Actions (mode-specific behavior) ---
        match self.mode {
            Mode::Interactive => {
                self.prev_command = self.interactive.update(time, self.max_command_delta);
                Some(self.prev_command)
            }
            Mode::Background => self.background_update(),
            Mode::Idle => None,
        }
    }

    /// Implementation of the update while in background mode. Gradually reduce the
    /// command toward zero, and then switch to Idle mode once we get there.
    fn background_update(&mut self) -> Option<f64> {
        let raw_command = self.prev_command - self.max_command_delta;

        self.prev_command = if raw_command > 0.0 {
            raw_command
        } else {
            self.mode = Mode::Idle;
            0.0
        };
        Some(self.prev_command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use more_asserts::{assert_ge, assert_le};

    fn policy(period: f64, max_delta: f64) -> f64 {
        // Simple policy that uses both inputs:
        // base = 2 * period; nudge by 0.5 * max_delta to prove plumbing.
        2.0 * period + 0.5 * max_delta
    }

    #[test]
    fn construction_defaults() {
        let fsm = FiniteStateMachine::new(0.3, 0.1, policy);
        assert_eq!(fsm.mode(), Mode::Background);
        assert_relative_eq!(fsm.prev_command(), 0.3);
        assert!(!fsm.interactive_data().is_time_cached());
        assert_ge!(fsm.prev_command(), 0.0);
    }

    #[test]
    fn first_interactive_tick_returns_initial_and_caches_time() {
        let mut fsm = FiniteStateMachine::new(0.3, 0.1, policy);
        let out = fsm.update(1.0, true); // interactive
        assert_eq!(fsm.mode(), Mode::Interactive);
        assert!(fsm.interactive_data().is_time_cached());
        assert_relative_eq!(out.unwrap(), 0.3);
        assert_relative_eq!(fsm.prev_command(), 0.3);
    }

    #[test]
    fn interactive_period_computes_policy_command() {
        let mut fsm = FiniteStateMachine::new(0.3, 0.1, policy);
        let _ = fsm.update(1.0, true); // first interactive tick returns initial command
        // Next tick: period=0.1 => cmd = 2*0.1 + 0.5*0.1 = 0.2 + 0.05 = 0.25
        let out = fsm.update(1.1, true);
        assert_eq!(fsm.mode(), Mode::Interactive);
        assert_relative_eq!(out.unwrap(), 0.25);
        assert_relative_eq!(fsm.prev_command(), 0.25);
        assert_ge!(fsm.prev_command(), 0.0);
        assert_le!(fsm.prev_command(), 1.0); // stays within [0,1]
    }

    #[test]
    #[should_panic]
    fn interactive_nonmonotonic_time_panics() {
        let mut fsm = FiniteStateMachine::new(0.3, 0.2, policy);
        let _ = fsm.update(2.0, true); // first interactive tick
        // time went backwards -> period <= 0 triggers assert_gt! panic
        let _ = fsm.update(1.9, true);
    }

    #[test]
    #[should_panic]
    fn interactive_zero_period_panics() {
        let mut fsm = FiniteStateMachine::new(0.3, 0.2, policy);
        let _ = fsm.update(2.0, true);
        // Same timestamp => period == 0.0 -> panic
        let _ = fsm.update(2.0, true);
    }

    #[test]
    fn interactive_to_background_then_decay() {
        let mut fsm = FiniteStateMachine::new(0.3, 0.1, policy);
        let _ = fsm.update(1.0, true); // returns 0.3
        // period=0.2 => cmd=2*0.2+0.05=0.45
        let _ = fsm.update(1.2, true);
        assert_relative_eq!(fsm.prev_command(), 0.45);

        // Flag goes false -> transition to background and decay in same tick
        let out = fsm.update(1.3, false);
        assert_eq!(fsm.mode(), Mode::Background);
        assert_relative_eq!(out.unwrap(), 0.35); // 0.45 - 0.1
        assert_relative_eq!(fsm.prev_command(), 0.35);

        // Decay again (still background)
        let out2 = fsm.update(1.4, false);
        assert_eq!(fsm.mode(), Mode::Background);
        assert_relative_eq!(out2.unwrap(), 0.25);
        assert_relative_eq!(fsm.prev_command(), 0.25);
    }

    #[test]
    fn background_to_idle_on_zero_crossing() {
        let mut fsm = FiniteStateMachine::new(0.15, 0.1, policy);
        assert_eq!(fsm.mode(), Mode::Background);

        // 0.15 -> 0.05
        let out1 = fsm.update(10.0, false);
        assert_eq!(fsm.mode(), Mode::Background);
        assert_relative_eq!(out1.unwrap(), 0.05);

        // 0.05 -> 0.00, transitions to Idle and returns a final 0.0 once
        let out2 = fsm.update(11.0, false);
        assert_eq!(fsm.mode(), Mode::Idle);
        assert_relative_eq!(out2.unwrap(), 0.0);
        assert_relative_eq!(fsm.prev_command(), 0.0);
    }

    #[test]
    fn idle_returns_none_until_interactive() {
        let mut fsm = FiniteStateMachine::new(0.05, 0.1, policy);
        // Background -> immediate decay to 0 -> Idle
        let _ = fsm.update(0.0, false); // 0.05 -> 0.0 & idle
        assert_eq!(fsm.mode(), Mode::Idle);

        // Idle returns None (no command)
        let out_none = fsm.update(1.0, false);
        assert!(out_none.is_none());
        assert_relative_eq!(fsm.prev_command(), 0.0);

        // Any-state -> Interactive when flag set
        let out_interactive = fsm.update(2.0, true);
        assert_eq!(fsm.mode(), Mode::Interactive);
        // First interactive tick returns the initial command (0.05)
        assert_relative_eq!(out_interactive.unwrap(), 0.05);
        assert_relative_eq!(fsm.prev_command(), 0.05);
    }

    #[test]
    fn reenter_interactive_returns_last_interactive_command() {
        let mut fsm = FiniteStateMachine::new(0.3, 0.1, policy);
        // First interactive tick -> 0.3
        let _ = fsm.update(1.0, true);
        // Second interactive tick: period=0.2 => 0.45
        let _ = fsm.update(1.2, true);
        assert_relative_eq!(fsm.prev_command(), 0.45);

        // Leave to background: reset stores 0.45 for next entry
        let _ = fsm.update(1.3, false); // decays output to 0.35, but reset kept 0.45
        assert_eq!(fsm.mode(), Mode::Background);

        // Re-enter interactive: should return stored 0.45 (not the decayed 0.35)
        let out = fsm.update(2.0, true);
        assert_eq!(fsm.mode(), Mode::Interactive);
        assert_relative_eq!(out.unwrap(), 0.45);
        assert_relative_eq!(fsm.prev_command(), 0.45);
    }
}
