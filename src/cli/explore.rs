use image::Rgb;
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
        file_io::{date_time_string, serialize_to_json_or_panic, FilePrefix},
        image_utils::{
            create_buffer, generate_scalar_image_in_place, write_image_to_file_or_panic,
            ImageSpecification, PixelMapper, PointRenderFn, Renderable,
        },
    },
    fractals::common::FractalParams,
};

// Parameters for GUI key-press interactions
const VIEW_FRACTION_STEP_PER_KEY_PRESS: f32 = 0.05;
const ZOOM_SCALE_FACTOR_PER_KEY_PRESS: f32 = 0.05;
const KEY_PRESS_SENSITIVITY_MODIFIER: f32 = 1.2;

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

    // Read the parameters file here. For now, only support Mandelbrot set.
    let (pixel_renderer, image_spec) = match params {
        FractalParams::Mandelbrot(inner_params) => {
            file_prefix.create_and_step_into_sub_directory("mandelbrot");
            let renderer = inner_params.clone().point_renderer();
            (renderer, inner_params.image_specification().clone())
        }
        _ => {
            println!("ERROR:  Unsupported fractal parameter type. Aborting.");
            panic!();
        }
    };

    let window = {
        let logical_size = LogicalSize::new(
            image_spec.resolution[0] as f64,
            image_spec.resolution[1] as f64,
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
            image_spec.resolution[0],
            image_spec.resolution[1],
            surface_texture,
        )?
    };

    let mut keyboard_action_effect_modifier = 1.0f32;

    // TODO:  move this up into the match branch, and then dynamic dispatch on the grid type
    // Then properly set up the image resolution here
    let mut pixel_grid = PixelGrid::new(file_prefix, image_spec, &pixel_renderer);

    // GUI application main loop:
    event_loop.run(move |event, _, control_flow| {
        // The one and only event that winit_input_helper doesn't have for us...
        if let Event::RedrawRequested(_) = event {
            pixel_grid.draw(pixels.frame_mut());
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

            // Zoom control --> W and S keys
            if input.key_pressed(VirtualKeyCode::W) {
                pixel_grid
                    .zoom(1.0 - keyboard_action_effect_modifier * ZOOM_SCALE_FACTOR_PER_KEY_PRESS);
            }
            if input.key_pressed(VirtualKeyCode::S) {
                pixel_grid
                    .zoom(1.0 + keyboard_action_effect_modifier * ZOOM_SCALE_FACTOR_PER_KEY_PRESS);
            }

            // Pan control --> arrow keys
            if input.key_pressed(VirtualKeyCode::Up) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    0f32,
                    keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                ));
            }
            if input.key_pressed(VirtualKeyCode::Down) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    0f32,
                    -keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                ));
            }
            if input.key_pressed(VirtualKeyCode::Left) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    -keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                    0f32,
                ));
            }
            if input.key_pressed(VirtualKeyCode::Right) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                    0f32,
                ));
            }

            // Pan/Zoom sensitivity --> A and D keys
            if input.key_pressed(VirtualKeyCode::A) {
                keyboard_action_effect_modifier /= KEY_PRESS_SENSITIVITY_MODIFIER;
            }
            if input.key_pressed(VirtualKeyCode::D) {
                keyboard_action_effect_modifier *= KEY_PRESS_SENSITIVITY_MODIFIER;
            }

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

            // Recenter the window on the mouse click location.
            if input.mouse_pressed(0) {
                let pixel_mapper = PixelMapper::new(&pixel_grid.image_specification);
                let point = pixel_mapper.map(&mouse_click_coordinates);
                pixel_grid.recenter(&nalgebra::Vector2::new(point.0, point.1));
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                if pixels.resize_surface(size.width, size.height).is_err() {
                    println!("ERROR:  unable to resize surface. Aborting.");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            if pixel_grid.update_required {
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }

            if input.key_pressed_os(VirtualKeyCode::Space) {
                pixel_grid.render_to_file();
            }
        }
    });
}
