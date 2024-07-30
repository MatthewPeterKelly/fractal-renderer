#![deny(clippy::all)]
#![forbid(unsafe_code)]

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
        file_io::{build_output_path_with_date_time, date_time_string, FilePrefix},
        histogram::{CumulativeDistributionFunction, Histogram},
        image_utils::{
            create_buffer, generate_scalar_image_in_place, ImageSpecification, PixelMapper,
        },
    },
    fractals::{
        common::FractalParams,
        mandelbrot::{
            create_color_map_black_blue_white, insert_buffer_into_histogram,
            mandelbrot_pixel_renderer,
        },
    },
};

// TODO:  docs
// TOOD:  move this to a parameter file of sorts?
const VIEW_FRACTION_STEP_PER_KEY_PRESS: f32 = 0.05;
const ZOOM_SCALE_FACTOR_PER_KEY_PRESS: f32 = 0.05;
const KEY_PRESS_JUMP_MODIFIER_SCALE: f32 = 1.2;

// Minimal rendering window example. Modulo index as color.

pub fn explore_fractal(params: &FractalParams) -> Result<(), Error> {
    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();

    // Read the parameters file here
    let (pixel_renderer, image_spec, mut histogram) = match params {
        FractalParams::Mandelbrot(inner_params) => (
            mandelbrot_pixel_renderer(inner_params),
            inner_params.image_specification.clone(),
            Histogram::new(
                inner_params.histogram_bin_count,
                inner_params.max_iter_count as f32,
            ),
        ),
        _ => {
            panic!(); // TODO:   proper error handling
        }
    };

    // TODO:  consider dropping the resolution if it is larger than the smallest screen size?

    let window = {
        let size = LogicalSize::new(
            image_spec.resolution[0] as f64,
            image_spec.resolution[1] as f64,
        );
        // Suspicious:
        let scaled_size = LogicalSize::new(
            image_spec.resolution[0] as f64,
            image_spec.resolution[1] as f64,
        );
        WindowBuilder::new()
            .with_title("Fractal Explorer")
            .with_inner_size(scaled_size)
            .with_min_inner_size(size)
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

    // Then properly set up the image resolution here
    let mut pixel_grid = PixelGrid::new(&image_spec, &pixel_renderer);

    // Allocate memory for color mapping:
    let color_map = create_color_map_black_blue_white();

    // GUI application main loop:
    event_loop.run(move |event, _, control_flow| {
        // The one and only event that winit_input_helper doesn't have for us...
        if let Event::RedrawRequested(_) = event {
            pixel_grid.draw(&color_map, &mut histogram, pixels.frame_mut());
            if  pixels.render().is_err() {
                println!("INFO:  ERROR:  unable to render pixels. Aborting.");
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

            // Action modifier --> A and D keys
            if input.key_pressed(VirtualKeyCode::A) {
                keyboard_action_effect_modifier /= KEY_PRESS_JUMP_MODIFIER_SCALE;
                println!("INFO:  Action modified: {:?}", keyboard_action_effect_modifier);
            }
            if input.key_pressed(VirtualKeyCode::D) {
                keyboard_action_effect_modifier *= KEY_PRESS_JUMP_MODIFIER_SCALE;
                println!("INFO:  Action modified: {:?}", keyboard_action_effect_modifier);
            }

            // Zoom control --> W and S keys
            if input.key_pressed(VirtualKeyCode::W) {
                pixel_grid
                    .zoom(1.0 - keyboard_action_effect_modifier * ZOOM_SCALE_FACTOR_PER_KEY_PRESS);
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }
            if input.key_pressed(VirtualKeyCode::S) {
                pixel_grid
                    .zoom(1.0 + keyboard_action_effect_modifier * ZOOM_SCALE_FACTOR_PER_KEY_PRESS);
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }

            // Pan control --> arrow keys
            if input.key_pressed(VirtualKeyCode::Up) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    0f32,
                    keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                ));
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }
            if input.key_pressed(VirtualKeyCode::Down) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    0f32,
                    -keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                ));
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }
            if input.key_pressed(VirtualKeyCode::Left) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    -keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                    0f32,
                ));
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }
            if input.key_pressed(VirtualKeyCode::Right) {
                pixel_grid.pan_view(&nalgebra::Vector2::<f32>::new(
                    keyboard_action_effect_modifier * VIEW_FRACTION_STEP_PER_KEY_PRESS,
                    0f32,
                ));
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }

            // Figure out where the mouse click happened.
            let mouse_cell = input
                .mouse()
                .map(|(mx, my)| {
                    let (mx_i, my_i) = pixels
                        .window_pos_to_pixel((mx, my))
                        .unwrap_or_else(|pos| pixels.clamp_pixel_pos(pos));

                    (mx_i as u32, my_i as u32)
                })
                .unwrap_or_default();

            // TODO:  this one only kind of works...
            if input.mouse_pressed(0) {
                let pixel_mapper = PixelMapper::new(&pixel_grid.image_specification);
                let point = pixel_mapper.map(&mouse_cell);
                // println!("INFO:  Mouse left-click at {mouse_cell:?} -->  {point:?}");
                pixel_grid.recenter(&nalgebra::Vector2::new(point.0, point.1));

                // TODO:  these following lines keep showing up...
                // Make an easier way to do this -- basically some "cache is dirty" flag on any method
                // that touches the parameters.
                pixel_grid.update(&pixel_renderer);
                window.request_redraw();
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                if pixels.resize_surface(size.width, size.height).is_err() {
                println!("INFO:  ERROR:  unable to resize surface. Aborting.");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            if input.key_pressed_os(VirtualKeyCode::Space) {
                // TODO:  need the full params file here.
                pixel_grid.render_to_file(&color_map, &mut histogram);
            }
        }
    });
}

#[derive(Clone, Debug)]
struct PixelGrid {
    image_specification: ImageSpecification,
    display_buffer: Vec<Vec<f32>>, // rendered to the screen on `draw()`
    scratch_buffer: Vec<Vec<f32>>, // updated in-place on `update()`
}

impl PixelGrid {
    fn new<F>(image_specification: &ImageSpecification, pixel_renderer: F) -> Self
    where
        F: Fn(&nalgebra::Vector2<f64>) -> f32 + std::marker::Sync,
    {
        let mut grid = Self {
            image_specification: image_specification.clone(),
            display_buffer: create_buffer(0f32, &image_specification.resolution),
            scratch_buffer: create_buffer(0f32, &image_specification.resolution),
        };
        grid.update(&pixel_renderer);
        grid.update(&pixel_renderer);
        grid
    }

    fn recenter(&mut self, center: &nalgebra::Vector2<f64>) {
        self.image_specification.center = *center;
        println!("INFO:  Recenter: {center:?}");
    }

    /**
     *  Computes the fractal; stored in a double buffer.
     */
    fn update<F>(&mut self, pixel_renderer: F)
    where
        F: Fn(&nalgebra::Vector2<f64>) -> f32 + std::marker::Sync,
    {
        generate_scalar_image_in_place(
            &self.image_specification,
            pixel_renderer,
            &mut self.scratch_buffer,
        );
        std::mem::swap(&mut self.scratch_buffer, &mut self.display_buffer);
        println!("INFO:  UPDATE CALLED!");
    }

    /**
     *  Renders data from the double buffer to the screen.
     */
    fn draw<F>(&self, color_map: &F, histogram: &mut Histogram, screen: &mut [u8])
    where
        F: Fn(f32) -> image::Rgb<u8>,
    {
        histogram.clear();
        insert_buffer_into_histogram(&self.display_buffer, histogram);
        let cdf = CumulativeDistributionFunction::new(histogram); // TODO: rework this to not allocate

        // The screen buffer should be 4x the size of our buffer because it has RGBA channels
        // where as we only have a scalar channel that is mapped through a color map.
        debug_assert_eq!(
            screen.len(),
            (4 * self.image_specification.resolution[0] * self.image_specification.resolution[1])
                as usize
        );
        let array_skip = self.image_specification.resolution[0] as usize;
        for (flat_index, pixel) in screen.chunks_exact_mut(4).enumerate() {
            let j = flat_index / array_skip;
            let i = flat_index % array_skip;
            let raw_pixel = color_map(cdf.percentile(self.display_buffer[i][j]));
            let color = [raw_pixel[0], raw_pixel[1], raw_pixel[2], 255];
            pixel.copy_from_slice(&color);
        }
        println!("INFO:  Draw called!");
    }

    fn render_to_file<F>(&self, color_map: &F, histogram: &mut Histogram)
    where
        F: Fn(f32) -> image::Rgb<u8>,
    {
        let file_prefix = FilePrefix {
            directory_path: build_output_path_with_date_time(
                "explore",
                "debug",
                &Some(date_time_string()),
            ),
            file_base: "foobar".to_owned(),
        };
        std::fs::write(
            file_prefix.with_suffix(".json"),
            serde_json::to_string(&self.image_specification).unwrap(),
        )
        .expect("Unable to write file");

        // TODO:  cache this?
        let cdf = CumulativeDistributionFunction::new(histogram);

        // Save the image to a file, deducing the type from the file name
        // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
        let mut imgbuf = image::ImageBuffer::new(
            self.image_specification.resolution[0],
            self.image_specification.resolution[1],
        );

        // Iterate over the coordinates and pixels of the image
        for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
            *pixel = color_map(cdf.percentile(self.display_buffer[x as usize][y as usize]));
        }

        let render_path = file_prefix.with_suffix(".png");
        imgbuf.save(&render_path).unwrap();
        println!("INFO:  Wrote image file to: {}", render_path.display());
    }

    fn pan_view(&mut self, view_fraction: &nalgebra::Vector2<f32>) {
        let x_delta = view_fraction[0] as f64 * self.image_specification.width;
        let y_delta = view_fraction[1] as f64 * self.image_specification.height();
        self.recenter(&nalgebra::Vector2::new(
            self.image_specification.center[0] + x_delta,
            self.image_specification.center[1] + y_delta,
        ));
    }

    fn zoom(&mut self, scale: f32) {
        self.image_specification.width *= scale as f64;
        // TODO: no good -- we need to manually remember to call this...
        // Opens us up to bugs!
        // Lets make a method to collect things and avoid doing it wrong.
        println!("INFO:  Zoom rescale: {:?}", scale);
    }
}
