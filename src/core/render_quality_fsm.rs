//! Simple FSM (finite state machine) that is used to regulate the
//! render quality command for the render pipeline. While the user
//! is actively interacting with the system, we want to hit a target
//! frame rate, even if the render quality is low. However, once the
//! user stops interacting, then we need to quickly crank up the render
//! quality regardless of frame rate. Finally, once we've rendered at
//! high quality, we should shut down the render pipeline to conserve
//! resources (no need to spin at max CPU while idle...).

pub trait RenderQualityPolicy {
    const MAX_COMMAND: f64 = 1.0;
    const MIN_COMMAND: f64 = 0.0;
    const MIN_PERIOD: f64 = 0.0;

    /// @param previous_command: last render command that was completed
    /// @param measured_period: how long did that render command take to complete?
    /// @return: render quality command (0 = maximum quality; 1 = maximum speed)
    ///     out of bound commands will be clamped to [0,1]
    fn evaluate(&mut self, previous_command: f64, measured_period: f64) -> f64;

    fn clamp_command(command: f64) -> f64 {
        command.clamp(Self::MIN_COMMAND, Self::MAX_COMMAND)
    }
}

use more_asserts::{assert_ge, assert_le};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    BeginRendering,
    Interactive,
    Background,
    Idle,
}

#[derive(Debug, Clone)]
pub struct FiniteStateMachine<F, G>
where
    F: RenderQualityPolicy,
    G: RenderQualityPolicy,
{
    mode: Mode,                   // which mode are we in right now?
    begin_rendering_command: f64, // what is the command to send when we first start rendering?
    prev_render_command: f64,
    prev_update_time: f64,
    interactive_policy: F,
    background_policy: G,
}

impl<F, G> FiniteStateMachine<F, G>
where
    F: RenderQualityPolicy,
    G: RenderQualityPolicy,
{
    /// Create a new FSM for regularing the render quality.
    pub fn new(initial_command: f64, interactive_policy: F, background_policy: G) -> Self {
        assert_ge!(initial_command, 0.0);
        assert_le!(initial_command, 1.0);
        let initial_command = initial_command.clamp(0.0, 1.0);
        Self {
            mode: Mode::BeginRendering,
            begin_rendering_command: initial_command,
            prev_render_command: initial_command,
            prev_update_time: 0.0,
            interactive_policy,
            background_policy,
        }
    }

    fn update_begin_rendering(&mut self, is_interactive: bool) -> Option<f64> {
        println!("FSM:   begin rendering");
        if is_interactive {
            self.mode = Mode::Interactive;
        } else {
            self.mode = Mode::Background;
        }
        self.prev_render_command = self.begin_rendering_command;
        Some(self.prev_render_command)
    }

    fn update_interactive(&mut self, period: f64, is_interactive: bool) -> Option<f64> {
        println!("FSM:   interactive");
        if !is_interactive {
            self.mode = Mode::Background;
        }

        let raw_command = self
            .interactive_policy
            .evaluate(self.prev_render_command, period);
        self.prev_render_command = F::clamp_command(raw_command);
        Some(self.prev_render_command)
    }

    fn update_background(&mut self, period: f64, is_interactive: bool) -> Option<f64> {
        println!("FSM:   background");
        if is_interactive {
            self.mode = Mode::Interactive;
        }
        let raw_render_command = self
            .background_policy
            .evaluate(self.prev_render_command, period);
        if raw_render_command <= 0.0 {
            self.mode = Mode::Idle;
        }
        self.prev_render_command = G::clamp_command(raw_render_command);
        Some(self.prev_render_command)
    }

    fn update_idle(&mut self, is_interactive: bool) -> Option<f64> {
        println!("FSM:   idle");
        if is_interactive {
            self.mode = Mode::BeginRendering;
        }
        None
    }

    pub fn update(&mut self, time: f64, is_interactive: bool) -> Option<f64> {
        let period = time - self.prev_update_time;
        match self.mode {
            Mode::BeginRendering => self.update_begin_rendering(is_interactive),
            Mode::Interactive => self.update_interactive(period, is_interactive),
            Mode::Background => self.update_background(period, is_interactive),
            Mode::Idle => self.update_idle(is_interactive),
        }
    }
}

// TODO:  testing
