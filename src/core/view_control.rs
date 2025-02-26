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
/// - `zoom_rate: f64`
///   The rate at which the view zooms, units are in "natural log of width per second".
#[derive(PartialEq, Debug)]
pub struct ZoomVelocityCommand {
    pub zoom_direction: ScalarDirection,
    pub zoom_rate: f64, // dimensionless per second
}

impl ZoomVelocityCommand {
    pub fn zero() -> ZoomVelocityCommand {
        ZoomVelocityCommand {
            zoom_direction: ScalarDirection::Zero(),
            zoom_rate: 0.0,
        }
    }
}

/// Actively control the center (panning) velocity.
/// Sending this command clears out any target command.
#[derive(PartialEq, Debug)]
pub struct CenterVelocityCommand {
    pub center_direction: [ScalarDirection; 2],
    pub pan_rate: f64,
}

impl CenterVelocityCommand {
    pub fn zero() -> CenterVelocityCommand {
        CenterVelocityCommand {
            center_direction: [ScalarDirection::Zero(), ScalarDirection::Zero()],
            pan_rate: 0.0,
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
/// - `pan_rate: f64`
///   The rate at which the view pans, measured in units of **view widths per second**.
///   This determines how quickly the center of the view moves horizontally and vertically
///   in response to panning commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CenterTargetCommand {
    pub view_center: [f64; 2],
    pub pan_rate: f64,
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
    // State:
    pub image_specification: ImageSpecification,
    pub initial_image_specification: ImageSpecification,

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
    /// - `image_specification: ImageSpecification`
    ///   Specify the initial view and resolution. The resolution will remain constant, but
    ///   view commands will alter the center and width of the view.
    ///
    pub fn new(time: f64, image_specification: &ImageSpecification) -> Self {
        Self {
            image_specification: *image_specification,
            initial_image_specification: *image_specification,
            pan_control: [
                PointTracker::new(time, image_specification.center[0]),
                PointTracker::new(time, image_specification.center[1]),
            ],
            zoom_control: PointTracker::new(time, image_specification.width.ln()),
        }
    }

    pub fn reset(&mut self) {
        self.image_specification = self.initial_image_specification;
        self.pan_control[0].set_position(self.image_specification.center[0]);
        self.pan_control[1].set_position(self.image_specification.center[1]);
        self.zoom_control
            .set_position(self.image_specification.width.ln());
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

    /// Updates the view control by applying the center and zoom commands to the
    /// simulated dynamics of the view onto the fractal.
    ///
    /// Normalize the pan rate by the width --> invariant over zoom scales.
    ///
    /// @return: true iff the update caused the view (center or scale) to change.
    pub fn update(
        &mut self,
        time: f64,
        center_command: CenterCommand,
        zoom_command: ZoomVelocityCommand,
    ) -> bool {
        match center_command {
            CenterCommand::Velocity(velocity_command) => {
                // We want consistent aparent velocity, so normalize the vector speed:
                let max_vel_vec = if velocity_command == CenterVelocityCommand::zero() {
                    [0.0, 0.0]
                } else {
                    compute_directional_max_velocity(
                        Vector2::from(velocity_command.vector_direction()),
                        velocity_command.pan_rate * self.image_specification.width,
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
                // Adjust the per-axis limits to enforce the max perceived speed:
                let max_vel_vec = compute_directional_max_velocity(
                    Vector2::from(center_target.view_center) - Vector2::from(self.view_center()),
                    center_target.pan_rate * self.image_specification.width,
                );

                for (index, max_vel) in max_vel_vec.iter().enumerate() {
                    self.pan_control[index].set_target(Target::Position {
                        pos_ref: center_target.view_center[index],
                        max_vel: *max_vel,
                    });
                }
            }
            CenterCommand::Idle {} => {
                for ctrl in &mut self.pan_control {
                    ctrl.set_idle_target();
                }
            }
        }

        self.zoom_control.set_target(Target::Velocity {
            vel_ref: zoom_command
                .zoom_direction
                .apply_to_magnitude(zoom_command.zoom_rate),
        });

        let mut view_was_modified = false;
        let mut monitored_assignment = |prev_value: &mut f64, next_value: f64| {
            if *prev_value != next_value {
                view_was_modified = true;
                *prev_value = next_value;
            }
        };

        monitored_assignment(
            &mut self.image_specification.center[0],
            self.pan_control[0].update_and_return_pos(time),
        );
        monitored_assignment(
            &mut self.image_specification.center[1],
            self.pan_control[1].update_and_return_pos(time),
        );
        monitored_assignment(
            &mut self.image_specification.width,
            self.zoom_control.update_and_return_pos(time).exp(),
        );
        view_was_modified
    }
}
