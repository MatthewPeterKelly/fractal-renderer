use crate::core::render_quality_fsm::{self, ConstantFrameRatePolicy, FiniteStateMachine};

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

////////////////////////////////////////////////////////////////////////////////////////

/// The `AdaptiveOptimizationRegulator` is a simple class wrapping a finite state machine
/// that is used to compute the "render quality" (0 = high quality but slow, 1 = low quality
/// but fast), while exploring a fractal interactively with the user.
#[derive(Clone, Debug)]
pub struct AdaptiveOptimizationRegulator {
    render_policy_fsm:
        render_quality_fsm::FiniteStateMachine<ConstantFrameRatePolicy, ConstantFrameRatePolicy>,
    render_start_time: Option<f64>,
    render_period: Option<f64>,
    render_command: Option<f64>,
}

/// For now, keep the regulator simple with some hard-coded policies.
/// Eventually these will be replaced with policies that depend on the
/// measured frame rate data.
impl Default for AdaptiveOptimizationRegulator {
    fn default() -> Self {
        Self {
            render_policy_fsm: FiniteStateMachine::new(
                0.0,
                ConstantFrameRatePolicy::new(0.55),
                ConstantFrameRatePolicy::new(0.0),
            ),
            render_start_time: None,
            render_period: None,
            render_command: None,
        }
    }
}

impl AdaptiveOptimizationRegulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.render_policy_fsm.reset();
        self.render_start_time = None;
        self.render_period = None;
        self.render_command = None;
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
        // `begin_rendering` call because the redrawing can take a long time.
        // For this reason, we guard the update here, only updating the data
        // on the first time that finish is called after begin.
        if let Some(start_time) = self.render_start_time {
            self.render_period = Some(time - start_time);
            self.render_start_time = None;
        }
    }
}
