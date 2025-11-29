use std::any::type_name;

use pixels::{Error, Pixels, SurfaceTexture};
use winit::{
    dpi::LogicalSize,
    event::{Event, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use winit_input_helper::WinitInputHelper;

use crate::{
    core::{
        file_io::FilePrefix,
        image_utils::{ImageSpecification, PixelMapper},
        render_window::{PixelGrid, RenderWindow},
        stopwatch::Stopwatch,
        view_control::{
            CenterCommand, CenterTargetCommand, CenterVelocityCommand, ScalarDirection,
            ViewControl, ZoomVelocityCommand,
        },
    },
    fractals::{common::FractalParams, quadratic_map::QuadraticMap},
};

const ZOOM_RATE: f64 = 0.4; // dimensionless. See `ViewControl` docs.
const FAST_ZOOM_RATE: f64 = 4.0 * ZOOM_RATE; // faster zoom option.
const PAN_RATE: f64 = 0.2; // window width per second
const FAST_PAN_RATE: f64 = 2.5 * PAN_RATE; // window width per second; used for "click to go".

fn direction_from_key_pair(neg_flag: bool, pos_flag: bool) -> ScalarDirection {
    if neg_flag == pos_flag {
        ScalarDirection::Zero()
    } else if pos_flag {
        ScalarDirection::Pos()
    } else {
        ScalarDirection::Neg()
    }
}

fn zoom_velocity_command_from_key_press(input: &WinitInputHelper) -> ZoomVelocityCommand {
    // Zoom control --> W and S keys
    let direction = direction_from_key_pair(
        input.key_held(VirtualKeyCode::W),
        input.key_held(VirtualKeyCode::S),
    );
    if direction == ScalarDirection::Zero() {
        // See if the user is doing a "fast zoom" instead:
        return ZoomVelocityCommand {
            zoom_direction: direction_from_key_pair(
                input.key_held(VirtualKeyCode::D),
                input.key_held(VirtualKeyCode::A),
            ),
            zoom_rate: FAST_ZOOM_RATE,
        };
    }

    ZoomVelocityCommand {
        zoom_direction: direction,
        zoom_rate: ZOOM_RATE,
    }
}

fn view_center_command_from_key_press(input: &WinitInputHelper) -> CenterCommand {
    // Pan control:  arrow keys
    let pan_up_down_command = direction_from_key_pair(
        input.key_held(VirtualKeyCode::Down),
        input.key_held(VirtualKeyCode::Up),
    );
    let pan_left_right_command = direction_from_key_pair(
        input.key_held(VirtualKeyCode::Left),
        input.key_held(VirtualKeyCode::Right),
    );

    let center_velocity = CenterVelocityCommand {
        center_direction: [pan_left_right_command, pan_up_down_command],
        pan_rate: PAN_RATE,
    };

    // If the user gave no input, then interpret this as "Idle", not "immediately stop".
    if center_velocity.center_direction == [ScalarDirection::Zero(), ScalarDirection::Zero()] {
        CenterCommand::Idle()
    } else {
        CenterCommand::Velocity(center_velocity)
    }
}

fn view_center_command_from_user_input(
    input: &WinitInputHelper,
    pixels: &Pixels,
    image_specification: &ImageSpecification,
) -> CenterCommand {
    // Check for mouse click --> used to command a view target
    if input.mouse_pressed(0) {
        // Figure out where the mouse click happened.
        let mouse_click_coordinates = input
            .mouse()
            .map(|(mx, my)| {
                let (mx_i, my_i) = pixels
                    .window_pos_to_pixel((mx, my))
                    .unwrap_or_else(|pos| pixels.clamp_pixel_pos(pos));
                (mx_i as u32, my_i as u32)
            })
            .unwrap_or_default();

        let pixel_mapper = PixelMapper::new(image_specification);
        let point = pixel_mapper.map(&mouse_click_coordinates);
        CenterCommand::Target(CenterTargetCommand {
            view_center: [point.0, point.1],
            pan_rate: FAST_PAN_RATE,
        })
    } else {
        // No mouse click, so let's see if the user wants to pan/zoom with the keyboard:
        view_center_command_from_key_press(input)
    }
}

fn reset_command_from_key_press(input: &WinitInputHelper) -> bool {
    if input.key_held(VirtualKeyCode::R) {
        return true;
    }
    if input.key_pressed(VirtualKeyCode::R) {
        return true;
    }
    false
}

/**
 * Create a simple GUI window that can be used to explore a fractal.
 * Supported features:
 * -- arrow keys for pan control
 * -- W/S keys for zoom control
 * -- mouse left click to recenter the image
 * -- A/D keys to adjust pan/zoom sensitivity
 */
pub fn explore_fractal(params: &FractalParams, mut file_prefix: FilePrefix) -> Result<(), Error> {
    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();
    let stopwatch = Stopwatch::new("Fractal Explorer".to_string());

    // Read the parameters file here and convert it into a `RenderWindow`.
    let time = stopwatch.total_elapsed_seconds();
    let mut render_window: Box<dyn RenderWindow> = match params {
        FractalParams::Mandelbrot(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("mandelbrot");
            Box::new(PixelGrid::new(
                stopwatch.total_elapsed_seconds(),
                file_prefix,
                ViewControl::new(time, &inner_params.image_specification),
                QuadraticMap::new((**inner_params).clone()),
            ))
        }

        FractalParams::Julia(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("julia");
            Box::new(PixelGrid::new(
                stopwatch.total_elapsed_seconds(),
                file_prefix,
                ViewControl::new(time, &inner_params.image_specification),
                QuadraticMap::new((**inner_params).clone()),
            ))
        }

        FractalParams::DrivenDampedPendulum(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("driven_damped_pendulum");
            Box::new(PixelGrid::new(
                stopwatch.total_elapsed_seconds(),
                file_prefix,
                ViewControl::new(time, &inner_params.image_specification),
                (**inner_params).clone(),
            ))
        }

        FractalParams::NewtonsMethod(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("newtons_method");
            Box::new(PixelGrid::new(
                stopwatch.total_elapsed_seconds(),
                file_prefix,
                ViewControl::new(time, &inner_params.image_specification),
                // MPK:  this constructor does not yet exist... But it shuold!
                // There is some tricky business to deal with around types. But solvable.
                NewtonsMethodRenderable::new((**inner_params).clone()),
            ))
        }

        _ => {
            println!(
                "ERROR: Parameter type `{}` does not yet implement the `RenderWindow` trait!  Aborting.",
                type_name::<FractalParams>()
            );
            panic!();
        }
    };

    let window = {
        let logical_size = LogicalSize::new(
            render_window.image_specification().resolution[0] as f64,
            render_window.image_specification().resolution[1] as f64,
        );
        WindowBuilder::new()
            .with_title("Fractal Explorer")
            .with_inner_size(logical_size)
            .with_min_inner_size(logical_size)
            .build(&event_loop)
            .unwrap()
    };

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Pixels::new(
            render_window.image_specification().resolution[0],
            render_window.image_specification().resolution[1],
            surface_texture,
        )?
    };

    // GUI application main loop:
    event_loop.run(move |event, _, control_flow| {
        // The one and only event that winit_input_helper doesn't have for us...
        if let Event::RedrawRequested(_) = event {
            render_window.draw(pixels.frame_mut());
            if pixels.render().is_err() {
                println!("ERROR:  unable to render pixels. Aborting.");
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        // For everything else, for let winit_input_helper collect events to build its state.
        // It returns `true` when it is time to update our game state and request a redraw.
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.close_requested() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            let center_command = view_center_command_from_user_input(
                &input,
                &pixels,
                render_window.image_specification(),
            );

            let zoom_command = zoom_velocity_command_from_key_press(&input);

            // Resize the window
            if let Some(size) = input.window_resized() {
                if pixels.resize_surface(size.width, size.height).is_err() {
                    println!("ERROR:  unable to resize surface. Aborting.");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            // Check for reset requests
            if reset_command_from_key_press(&input) {
                render_window.reset();
            }

            // Now do the hard rendering math:
            if render_window.update(
                stopwatch.total_elapsed_seconds(),
                center_command,
                zoom_command,
            ) {
                window.request_redraw();
            }

            if input.key_pressed_os(VirtualKeyCode::Space) {
                render_window.render_to_file();
            }
        }
    });
}
