use pixels::{Error, Pixels, SurfaceTexture};
use std::time::{Duration, Instant};
use std::{collections::HashSet, env, fs};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::core::{
    file_io::FilePrefix,
    image_utils::{ImageSpecification, PixelMapper, Renderable},
    render_window::{PixelGrid, RenderWindow},
    stopwatch::Stopwatch,
    view_control::{
        CenterCommand, CenterTargetCommand, CenterVelocityCommand, ScalarDirection, ViewControl,
        ZoomVelocityCommand,
    },
};

const ZOOM_RATE: f64 = 0.4; // dimensionless. See `ViewControl` docs.
const FAST_ZOOM_RATE: f64 = 4.0 * ZOOM_RATE; // faster zoom option.
const PAN_RATE: f64 = 0.2; // window width per second
const FAST_PAN_RATE: f64 = 2.5 * PAN_RATE; // window width per second; used for "click to go".
                                           // While rendering or when keys are held, wake periodically to keep interaction smooth
                                           // without busy-spinning the event loop.
const ACTIVE_LOOP_TICK_MS: u64 = 10;

#[derive(Default)]
struct RawInputState {
    held_keys: HashSet<VirtualKeyCode>,
    pressed_keys_this_frame: HashSet<VirtualKeyCode>,
    mouse_left_pressed_this_frame: bool,
    last_cursor_position: Option<(f64, f64)>,
}

impl RawInputState {
    fn observe_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { input, .. } => {
                if let Some(keycode) = input.virtual_keycode {
                    match input.state {
                        ElementState::Pressed => {
                            self.held_keys.insert(keycode);
                            self.pressed_keys_this_frame.insert(keycode);
                        }
                        ElementState::Released => {
                            self.held_keys.remove(&keycode);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_position = Some((position.x, position.y));
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.mouse_left_pressed_this_frame = true;
            }
            WindowEvent::Focused(false) => {
                self.held_keys.clear();
                self.pressed_keys_this_frame.clear();
                self.mouse_left_pressed_this_frame = false;
                self.last_cursor_position = None;
            }
            _ => {}
        }
    }

    fn key_held(&self, key: VirtualKeyCode) -> bool {
        self.held_keys.contains(&key)
    }

    fn key_pressed_this_frame(&self, key: VirtualKeyCode) -> bool {
        self.pressed_keys_this_frame.contains(&key)
    }

    fn has_active_keys(&self) -> bool {
        !self.held_keys.is_empty()
    }

    fn end_frame(&mut self) {
        self.pressed_keys_this_frame.clear();
        self.mouse_left_pressed_this_frame = false;
    }
}

fn running_in_wsl() -> bool {
    env::var_os("WSL_INTEROP").is_some()
        || fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|s| s.to_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn direction_from_key_pair(neg_flag: bool, pos_flag: bool) -> ScalarDirection {
    if neg_flag == pos_flag {
        ScalarDirection::Zero()
    } else if pos_flag {
        ScalarDirection::Pos()
    } else {
        ScalarDirection::Neg()
    }
}

fn zoom_velocity_command_from_key_press(raw: &RawInputState) -> ZoomVelocityCommand {
    // Zoom control --> W and S keys
    let direction = direction_from_key_pair(
        raw.key_held(VirtualKeyCode::W),
        raw.key_held(VirtualKeyCode::S),
    );
    if direction == ScalarDirection::Zero() {
        // See if the user is doing a "fast zoom" instead:
        return ZoomVelocityCommand {
            zoom_direction: direction_from_key_pair(
                raw.key_held(VirtualKeyCode::D),
                raw.key_held(VirtualKeyCode::A),
            ),
            zoom_rate: FAST_ZOOM_RATE,
        };
    }

    ZoomVelocityCommand {
        zoom_direction: direction,
        zoom_rate: ZOOM_RATE,
    }
}

fn view_center_command_from_key_press(raw: &RawInputState) -> CenterCommand {
    // Pan control:  arrow keys
    let pan_up_down_command = direction_from_key_pair(
        raw.key_held(VirtualKeyCode::Down),
        raw.key_held(VirtualKeyCode::Up),
    );
    let pan_left_right_command = direction_from_key_pair(
        raw.key_held(VirtualKeyCode::Left),
        raw.key_held(VirtualKeyCode::Right),
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
    raw: &RawInputState,
    pixels: &Pixels,
    image_specification: &ImageSpecification,
) -> CenterCommand {
    // Check for mouse click --> used to command a view target
    if raw.mouse_left_pressed_this_frame {
        // Figure out where the mouse click happened.
        let mouse_click_coordinates = raw
            .last_cursor_position
            .map(|(x, y)| (x as f32, y as f32))
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
        view_center_command_from_key_press(raw)
    }
}

fn reset_command_from_key_press(raw: &RawInputState) -> bool {
    raw.key_held(VirtualKeyCode::R) || raw.key_pressed_this_frame(VirtualKeyCode::R)
}

/**
 * Create a simple GUI window that can be used to explore a fractal.
 * Supported features:
 * -- arrow keys for pan control
 * -- W/S keys for zoom control
 * -- mouse left click to recenter the image
 * -- A/D keys to adjust pan/zoom sensitivity
 */
pub fn explore<F: Renderable + 'static>(
    file_prefix: FilePrefix,
    image_specification: ImageSpecification,
    renderer: F,
) -> Result<(), Error> {
    // Keep backend selection under user control and let winit auto-detect by default.
    if running_in_wsl() && env::var_os("WINIT_UNIX_BACKEND").is_none() {
        eprintln!(
            "Note: WSL detected; using winit auto backend selection. Set WINIT_UNIX_BACKEND=wayland or x11 to force a backend."
        );
    }

    // Create the event loop with a friendlier failure path.
    let event_loop = std::panic::catch_unwind(EventLoop::new)
        .unwrap_or_else(|p| {
            let msg = panic_message(p);
            eprintln!("\nERROR: Failed to initialize windowing backend.\n{msg}\n");

            if running_in_wsl() {
                eprintln!("WSL detected.");
                eprintln!("If you're forcing X11, you may be missing X11 runtime libs.");
                eprintln!("On Ubuntu, try: sudo apt install -y libxcursor1");
                eprintln!("(and ensure DISPLAY is set; WSLg usually sets it automatically.)");
            } else {
                eprintln!("Tip: ensure your system has either a working Wayland compositor or X11 libraries installed.");
            }

            std::process::exit(1);
        });

    let mut raw_input = RawInputState::default();
    let stopwatch = Stopwatch::new("Fractal Explorer".to_string());

    // Read the parameters file here and convert it into a `RenderWindow`.
    let time = stopwatch.total_elapsed_seconds();
    let mut render_window = PixelGrid::new(
        stopwatch.total_elapsed_seconds(),
        file_prefix,
        ViewControl::new(time, image_specification),
        renderer,
    );

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
        let should_tick = raw_input.has_active_keys()
            || render_window.render_task_is_busy()
            || render_window.redraw_required();
        *control_flow = if should_tick {
            ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(ACTIVE_LOOP_TICK_MS))
        } else {
            ControlFlow::Wait
        };

        if let Event::WindowEvent { event, .. } = &event {
            raw_input.observe_window_event(event);

            match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                WindowEvent::Resized(size) => {
                    if pixels.resize_surface(size.width, size.height).is_err() {
                        println!("ERROR:  unable to resize surface. Aborting.");
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
                _ => {}
            }
        }

        // Handle redraw requests from the windowing system.
        if let Event::RedrawRequested(_) = event {
            render_window.draw(pixels.frame_mut());
            if pixels.render().is_err() {
                println!("ERROR:  unable to render pixels. Aborting.");
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        if let Event::MainEventsCleared = event {
            // Close events
            if raw_input.key_pressed_this_frame(VirtualKeyCode::Escape)
                || raw_input.key_held(VirtualKeyCode::Escape)
            {
                *control_flow = ControlFlow::Exit;
                return;
            }

            let center_command = view_center_command_from_user_input(
                &raw_input,
                &pixels,
                render_window.image_specification(),
            );

            let zoom_command = zoom_velocity_command_from_key_press(&raw_input);

            // Check for reset requests
            if reset_command_from_key_press(&raw_input) {
                render_window.reset();
            }

            // Now do the hard rendering math:
            let redraw_required = render_window.update(
                stopwatch.total_elapsed_seconds(),
                center_command,
                zoom_command,
            );

            if redraw_required {
                window.request_redraw();
            }

            if raw_input.key_pressed_this_frame(VirtualKeyCode::Space) {
                render_window.render_to_file();
            }

            raw_input.end_frame();
        }
    });
}
