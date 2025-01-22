use nalgebra::Vector2;

use super::{
    controller::{PointTracker, Target},
    image_utils::ImageSpecification,
};

#[derive(PartialEq, Debug)]
pub enum ScalarDirection {
    Neg(),
    Zero(),
    Pos(),
}

impl ScalarDirection {
    pub fn apply_to_magnitude(&self, magnitude: f64) -> f64 {
        match self {
            ScalarDirection::Neg() => -magnitude,
            ScalarDirection::Zero() => 0.0,
            ScalarDirection::Pos() => magnitude,
        }
    }
}

/// Actively control the zoom velocity.
#[derive(PartialEq, Debug)]
pub struct ZoomVelocityCommand {
    pub zoom_direction: ScalarDirection,
}

impl ZoomVelocityCommand {
    pub fn zero() -> ZoomVelocityCommand {
        ZoomVelocityCommand {
            zoom_direction: ScalarDirection::Zero(),
        }
    }
}

/// Actively control the center (panning) velocity.
/// Sending this command clears out any target command.
#[derive(PartialEq, Debug)]
pub struct CenterVelocityCommand {
    pub center_direction: [ScalarDirection; 2],
}

impl CenterVelocityCommand {
    pub fn zero() -> CenterVelocityCommand {
        CenterVelocityCommand {
            center_direction: [ScalarDirection::Zero(), ScalarDirection::Zero()],
        }
    }

    pub fn vector_direction(&self) -> [f64; 2] {
        [
            self.center_direction[0].apply_to_magnitude(1.0),
            self.center_direction[1].apply_to_magnitude(1.0),
        ]
    }
}

/// Tell the view to servo to the specified target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CenterTargetCommand {
    pub view_center: [f64; 2],
}

/// Interface allowing the GUI to send simple commands to
/// the view control. Typically this is constructed by keyboard
/// button presses, so they are "boolean and mouse" parameters.
/// The center-panning rates are set in the `ViewControl` constructor.
#[derive(Debug, PartialEq)]
pub enum CenterCommand {
    Velocity(CenterVelocityCommand),
    Target(CenterTargetCommand),
    Idle(),
}

/// Given an expected pan velocity direction and max speed,
/// compute the per-axis limits that will enforce the max
/// speed along the specified direction.
pub fn compute_directional_max_velocity(direction: Vector2<f64>, max_speed: f64) -> [f64; 2] {
    const MIN_DIRECION_MAGNITUDE: f64 = 1e-12;
    let length = direction.magnitude();

    if length > MIN_DIRECION_MAGNITUDE {
        let scaled_pan_rate = (max_speed / length) * direction.abs();
        [scaled_pan_rate[0], scaled_pan_rate[1]]
    } else {
        [max_speed, max_speed]
    }
}

/// Note: the zoom rate is tricky. Here is how the math works:
///
/// view_width = (units) * alpha.exp();
/// alpha_next = alpha_prev + zoom_rate * dt;
///
/// The pan rate is way simpler:
///
/// center_next = center_prev + pan_rate * dt;
///
#[derive(Clone, Debug)]
pub struct ViewControl {
    // Parameters
    pub pan_rate: f64,  // view_width per second
    pub zoom_rate: f64, // dimensionless per second

    // State:
    pub image_specification: ImageSpecification,
    pub maybe_target_command: Option<CenterTargetCommand>,

    // Internal controllers:
    pub pan_control: [PointTracker; 2],
    pub zoom_control: PointTracker,
}

impl ViewControl {
    /// Creates a new instance of `ViewControl` used to control a "view" (ImageSpecification)
    /// as it evolves over time based on keyboard and mouse inputs.
    ///
    /// # Parameters
    /// - `time: f64`
    ///   The current time, used as the baseline for tracking animation or view updates.
    ///   Typically expressed in seconds since some reference point.
    ///
    /// - `pan_rate: f64`
    ///   The rate at which the view pans, measured in units of **view widths per second**.
    ///   This determines how quickly the center of the view moves horizontally and vertically
    ///   in response to panning commands.
    ///
    /// - `zoom_rate: f64`
    ///   The rate at which the view zooms, units are in "natural log of width per second".
    ///
    /// - `rise_time: f64`
    ///   Used to set the gains for how quickly the tracking servos respond to commands.
    ///
    /// - `image_specification: ImageSpecification`
    ///   Specify the initial view and resolution. The resolution will remain constant, but
    ///   view commands will alter the center and width of the view.
    ///
    pub fn new(
        time: f64,
        pan_rate: f64,
        zoom_rate: f64,
        image_specification: &ImageSpecification,
    ) -> Self {
        Self {
            pan_rate,
            zoom_rate,
            image_specification: image_specification.clone(),
            maybe_target_command: None,
            pan_control: [
                PointTracker::new(time, image_specification.center[0]),
                PointTracker::new(time, image_specification.center[1]),
            ],
            zoom_control: PointTracker::new(time, image_specification.width.ln()),
        }
    }

    pub fn view_center(&self) -> [f64; 2] {
        [
            self.pan_control[0].position(),
            self.pan_control[1].position(),
        ]
    }

    pub fn image_specification(&self) -> &ImageSpecification {
        &self.image_specification
    }

    pub fn update(
        &mut self,
        time: f64,
        center_command: CenterCommand,
        zoom_command: ZoomVelocityCommand,
    ) -> &ImageSpecification {
        // Normalize the pan rate by the width --> invariant over zoom scales.
        let pan_rate = self.pan_rate * self.image_specification.width;
        match center_command {
            CenterCommand::Velocity(velocity_command) => {
                self.maybe_target_command = None;
                // We want consistent aparent velocity, so normalize the vector speed:
                let max_vel_vec = if velocity_command == CenterVelocityCommand::zero() {
                    [0.0, 0.0]
                } else {
                    compute_directional_max_velocity(
                        Vector2::from(velocity_command.vector_direction()),
                        pan_rate,
                    )
                };
                for (index, max_vel) in max_vel_vec.iter().enumerate() {
                    self.pan_control[index].set_target(Target::Velocity {
                        vel_ref: velocity_command.center_direction[index]
                            .apply_to_magnitude(*max_vel),
                    });
                }
            }
            CenterCommand::Target(center_target) => {
                let next_target = Some(center_target);
                // Update the target only if a new one is received or we're actively zooming in.
                // Recompute on non-zero zoom since the pan rate depends on the width, which in
                // turn depends on the zoom rate.
                if self.maybe_target_command != next_target
                    || zoom_command != ZoomVelocityCommand::zero()
                {
                    self.maybe_target_command = next_target;
                    // Adjust the per-axis limits to enforce the max perceived speed:
                    let max_vel_vec = compute_directional_max_velocity(
                        Vector2::from(center_target.view_center)
                            - Vector2::from(self.view_center()),
                        pan_rate,
                    );

                    for (index, max_vel) in max_vel_vec.iter().enumerate() {
                        self.pan_control[index].set_target(Target::Position {
                            pos_ref: center_target.view_center[index],
                            max_vel: *max_vel,
                        });
                    }
                }
            }
            CenterCommand::Idle {} => match self.maybe_target_command {
                Some(target_view) => {
                    return self.update(time, CenterCommand::Target(target_view), zoom_command);
                }
                None => {
                    return self.update(
                        time,
                        CenterCommand::Velocity(CenterVelocityCommand::zero()),
                        zoom_command,
                    );
                }
            },
        }

        self.zoom_control.set_target(Target::Velocity {
            vel_ref: zoom_command
                .zoom_direction
                .apply_to_magnitude(self.zoom_rate),
        });

        self.image_specification.center[0] = self.pan_control[0].update_and_return_pos(time);
        self.image_specification.center[1] = self.pan_control[1].update_and_return_pos(time);
        self.image_specification.width = self.zoom_control.update_and_return_pos(time).exp();

        &self.image_specification
    }
}
