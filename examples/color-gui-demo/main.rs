//! Demo: color map editor GUI with a live Mandelbrot preview.
//!
//! Run with: `cargo run --example color-gui-demo`

use std::time::{Duration, Instant};

use pixels::{Pixels, SurfaceTexture};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use fractal_renderer::core::{
    color_map_editor_ui::{
        blit_preview_to_framebuffer, init_egui, render_editor_frame, update_screen_descriptor,
        EditorState, EguiRenderContext, PREVIEW_W, TOTAL_H, TOTAL_W,
    },
    image_utils::{create_buffer, Renderable},
};
use fractal_renderer::fractals::{common::FractalParams, quadratic_map::QuadraticMap};

/// Render a Mandelbrot fractal at PREVIEW_W x TOTAL_H into an RGB buffer.
fn render_mandelbrot_preview(
    params: &fractal_renderer::fractals::mandelbrot::MandelbrotParams,
) -> Vec<Vec<image::Rgb<u8>>> {
    let mut preview_params = params.clone();
    preview_params.image_specification.resolution = [PREVIEW_W, TOTAL_H];
    let renderer = QuadraticMap::new(preview_params);
    let mut buffer = create_buffer(image::Rgb([0u8, 0, 0]), &[PREVIEW_W, TOTAL_H]);
    renderer.render_to_buffer(&mut buffer);
    buffer
}

fn main() {
    let json_text =
        std::fs::read_to_string("examples/color-gui-demo/params.json").unwrap_or_else(|e| {
            panic!(
                "Failed to read params.json — run from the repo root with:\n  \
                 cargo run --example color-gui-demo\n\
                 Error: {}",
                e
            )
        });
    let fractal_params: FractalParams = serde_json::from_str(&json_text)
        .unwrap_or_else(|e| panic!("Failed to parse params.json: {}", e));

    let (mandelbrot_params, keyframes) = match fractal_params {
        FractalParams::Mandelbrot(params) => {
            let kf = params.color_map.keyframes.clone();
            (*params, kf)
        }
        _ => panic!("color-gui-demo expects Mandelbrot params"),
    };

    println!("Rendering Mandelbrot preview ({PREVIEW_W}x{TOTAL_H})...");
    let preview_buffer = render_mandelbrot_preview(&mandelbrot_params);
    println!("Preview ready. Opening window...");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Color Map Editor \u{2014} Demo")
        .with_inner_size(LogicalSize::new(TOTAL_W as f64, TOTAL_H as f64))
        .build(&event_loop)
        .unwrap();

    let mut pixels = {
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, &window);
        Pixels::new(TOTAL_W, TOTAL_H, surface).expect("failed to create Pixels")
    };

    blit_preview_to_framebuffer(&mut pixels, &preview_buffer);

    let (egui_ctx, mut egui_state, mut egui_renderer, mut screen_descriptor) =
        init_egui(&event_loop, &pixels);
    let mut editor_state = EditorState::default();

    event_loop.run(move |event, _, control_flow| {
        if let Event::NewEvents(StartCause::Init) = event {
            *control_flow = ControlFlow::Wait;
        }

        if let Event::WindowEvent {
            event: ref window_event,
            ..
        } = event
        {
            let response = egui_state.on_event(&egui_ctx, window_event);
            if response.consumed {
                window.request_redraw();
                return;
            }

            match window_event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    if input.state == ElementState::Pressed {
                        if let Some(VirtualKeyCode::Escape | VirtualKeyCode::Q) =
                            input.virtual_keycode
                        {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                    }
                }
                WindowEvent::Resized(size) => {
                    if pixels.resize_surface(size.width, size.height).is_err() {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    update_screen_descriptor(&mut screen_descriptor, &window);
                }
                _ => {}
            }
        }

        if let Event::RedrawRequested(_) = event {
            let mut egui_render = EguiRenderContext {
                ctx: &egui_ctx,
                state: &mut egui_state,
                renderer: &mut egui_renderer,
                screen_descriptor: &screen_descriptor,
            };
            match render_editor_frame(
                &mut pixels,
                &mut egui_render,
                &window,
                &mut editor_state,
                &keyframes,
            ) {
                Ok(repaint_after) => {
                    if repaint_after == Duration::ZERO {
                        window.request_redraw();
                    } else {
                        *control_flow = ControlFlow::WaitUntil(Instant::now() + repaint_after);
                    }
                }
                Err(_) => {
                    *control_flow = ControlFlow::Exit;
                }
            }
        }

        if let Event::MainEventsCleared = event {
            window.request_redraw();
        }
    });
}
