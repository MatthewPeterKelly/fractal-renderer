#[derive(Clone, Debug)]
pub enum Target {
    Position { pos_ref: f64, max_vel: f64 },
    Velocity { vel_ref: f64 },
}

/// TODO
/// @return: next position
pub fn update_position(position: f64, delta_time: f64, target: &Target) -> f64 {
    match *target {
        Target::Position { pos_ref, max_vel } => {
            let pos_err = pos_ref - position;
            let max_pos_delta = (max_vel * delta_time).abs();

            if max_pos_delta > pos_err.abs() {
                return pos_ref;
            }
            let pos_err_clamped = pos_err.clamp(-max_pos_delta, max_pos_delta);
            position + pos_err_clamped
        }
        Target::Velocity { vel_ref } => position + vel_ref * delta_time,
    }
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

    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn update_and_return_pos(&mut self, time: f64) -> f64 {
        let delta_time = time - self.time;
        self.time += delta_time;
        self.position = update_position(self.position, delta_time, &self.target);
        self.position
    }
}
