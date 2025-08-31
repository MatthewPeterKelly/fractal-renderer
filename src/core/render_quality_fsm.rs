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
    /// Create a new FSM for regularing the render quality.
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
        let prev_command = prev_command.expect("ERROR: period data was set, but there is no matching command!");
        let raw_render_command = self
            .background_policy
            .evaluate(prev_command, period);
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

// TODO:  testing
// ON transition from IDLE to Begin, we run with the last interactive command
