//! Simple FSM (finite state machine) that is used to regulate the
//! render quality command for the render pipeline. While the user
//! is actively interacting with the system, we want to hit a target
//! frame rate, even if the render quality is low. However, once the
//! user stops interacting, then we need to quickly crank up the render
//! quality regardless of frame rate. Finally, once we've rendered at
//! high quality, we should shut down the render pipeline to conserve
//! resources (no need to spin at max CPU while idle...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Interactive,
    Background,
    Idle,
}

/// Per-mode continuous data for the Interactive mode.
#[derive(Debug, Clone)]
pub struct InteractiveData<F>
where
    F: Fn(f64, f64) -> f64,
{
    prev_time: Option<f64>,
    command: f64,
    get_interactive_command: F,
}

impl<F> InteractiveData<F>
where
    F: Fn(f64, f64) -> f64,
{
    pub fn new(initial_command: f64, get_interactive_command: F) -> Self {
        // TODO: ensure on range [0,1] inclusive
        debug_assert!(initial_command.is_finite(), "initial_command must be finite");
        Self {
            prev_time: None,
            command: initial_command,
            get_interactive_command,
        }
    }

    // TODO:  add a reset method that is called on any transition out of this mode,
    // which unsets the previouis time, so that on the first tick in the mode it
    // will always just return the previous command.

        // TODO: unused
    pub fn command(&self) -> f64 { self.command }

    // TODO:  rename to update
    pub fn step(&mut self, time: f64, max_command_delta: f64) -> f64 {
        let cmd = if let Some(prev_t) = self.prev_time {
            let mut period = time - prev_t;
            if !period.is_finite() || period < 0.0 {
                period = 0.0;
            }
            (self.get_interactive_command)(period, max_command_delta)
        } else {
            self.command
        };

        self.prev_time = Some(time);
        self.command = cmd;
        cmd
    }
}

#[derive(Debug)]
pub struct Fsm<F>
where
    F: Fn(f64, f64) -> f64,
{
    mode: Mode,
    prev_command: f64,
    max_command_delta: f64,
    interactive: InteractiveData<F>,
}

impl<F> Fsm<F>
where
    F: Fn(f64, f64) -> f64,
{
    /// Create a new FSM.
    ///
    /// - `initial_command` initializes both the global `prev_command` and the
    ///   interactive state's `command`.
    /// - `max_command_delta` must be finite and >= 0.0.
    /// - `get_interactive_command(period, max_command_delta)` returns the interactive command.
    pub fn new(initial_command: f64, max_command_delta: f64, get_interactive_command: F) -> Self {
        debug_assert!(initial_command.is_finite(), "initial_command must be finite");
        debug_assert!(max_command_delta.is_finite(), "max_command_delta must be finite");
        debug_assert!(max_command_delta >= 0.0, "max_command_delta must be >= 0");

        Self {
            mode: Mode::Background,
            prev_command: initial_command,
            max_command_delta,
            interactive: InteractiveData::new(initial_command, get_interactive_command),
        }
    }

    pub fn mode(&self) -> Mode { self.mode }
    pub fn prev_command(&self) -> f64 { self.prev_command }
    pub fn interactive_data(&self) -> &InteractiveData<F> { &self.interactive }

    /// Update the render FSM and return the render command, if any is needed.
    pub fn update(&mut self, time: f64, is_interactive: bool) -> Option<f64> {
        debug_assert!(time.is_finite(), "time must be finite");

            // TODO: split up the transition and udpate of the FSM, each handled by a
            // match statement. The first match does the transitions (discrete mode
            // changes and reset logic), then the second performs actions.

        if is_interactive {
            // Enter/Remain Interactive
            self.mode = Mode::Interactive;
            let cmd = self.interactive.step(time, self.max_command_delta);
            self.prev_command = cmd; // FSM-wide last command
            return Some(cmd);
        }

        // Not interactive this tick
        if self.mode == Mode::Interactive {
            self.mode = Mode::Background;
        }

        match self.mode {
            Mode::Background => self.step_background(),
            Mode::Idle => None,
            Mode::Interactive => unreachable!("handled above"),
        }
    }

    /// Background behavior:
    /// - Decay `prev_command` by `max_command_delta`.
    /// - If still positive, stay in Background and return Some(next).
    /// - If non-positive, move to Idle and return a final Some(0.0) once.
    fn step_background(&mut self) -> Option<f64> {
        let mut next = self.prev_command - self.max_command_delta;
        if !next.is_finite() {
            next = 0.0;
        }

        if next > 0.0 {
            self.prev_command = next;
            Some(next)
        } else {
            self.mode = Mode::Idle;
            self.prev_command = 0.0;
            Some(0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use more_asserts::{assert_ge, assert_le};

    fn policy(period: f64, max_delta: f64) -> f64 {
        // Example policy that uses both inputs:
        // base = 2 * period; nudge by 0.5 * max_delta to show it's plumbed through.
        2.0 * period + 0.5 * max_delta
    }

    #[test]
    fn construction_defaults() {
        let fsm = Fsm::new(0.3, 0.1, policy);
        assert_eq!(fsm.mode(), Mode::Background);
        assert_relative_eq!(fsm.prev_command(), 0.3);
        assert!(fsm.interactive_data().prev_time().is_none());
        assert_relative_eq!(fsm.interactive_data().command(), 0.3);
        assert_ge!(fsm.prev_command(), 0.0);
    }

    #[test]
    fn first_interactive_tick_uses_previous_interactive_command() {
        let mut fsm = Fsm::new(0.3, 0.1, policy);
        let out = fsm.update(1.0, true); // interactive
        assert_eq!(fsm.mode(), Mode::Interactive);
        assert!(fsm.interactive_data().prev_time().is_some());
        assert_relative_eq!(out.unwrap(), 0.3);
        assert_relative_eq!(fsm.prev_command(), 0.3);
    }

    #[test]
    fn interactive_period_computes_policy_command() {
        let mut fsm = Fsm::new(0.3, 0.1, policy);
        let _ = fsm.update(1.0, true); // first interactive tick returns prior interactive cmd
        // Next tick: period=0.1 => cmd = 2*0.1 + 0.5*0.1 = 0.2 + 0.05 = 0.25
        let out = fsm.update(1.1, true);
        assert_eq!(fsm.mode(), Mode::Interactive);
        assert_relative_eq!(out.unwrap(), 0.25);
        assert_relative_eq!(fsm.interactive_data().command(), 0.25);
        assert_relative_eq!(fsm.prev_command(), 0.25);
        assert_ge!(fsm.prev_command(), 0.0);
        assert_le!(fsm.prev_command(), 10.0); // arbitrary sanity guard
    }

    #[test]
    fn interactive_nonmonotonic_time_is_clamped() {
        let mut fsm = Fsm::new(0.3, 0.2, policy);
        let _ = fsm.update(2.0, true); // first interactive tick
        // time went backwards -> period clamped to 0 => cmd = 2*0 + 0.5*0.2 = 0.1
        let out = fsm.update(1.9, true);
        assert_relative_eq!(out.unwrap(), 0.1);
        assert_relative_eq!(fsm.prev_command(), 0.1);
        assert_relative_eq!(fsm.interactive_data().command(), 0.1);
    }

    #[test]
    fn interactive_to_background_then_decay() {
        let mut fsm = Fsm::new(0.3, 0.1, policy);
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
        let mut fsm = Fsm::new(0.15, 0.1, policy);
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
        let mut fsm = Fsm::new(0.05, 0.1, policy);
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
        // First interactive tick uses previous interactive command (which was initial 0.05)
        assert_relative_eq!(out_interactive.unwrap(), 0.05);
        assert_relative_eq!(fsm.prev_command(), 0.05);
    }
}
