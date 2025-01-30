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
