/// Interactive color-map editor.
///
/// This module provides the `edit` entry point that opens a single window
/// containing a live fractal preview on the left and an editor panel on the
/// right.  Using one window avoids the requirement for a GPU that can drive
/// two wgpu surfaces simultaneously (e.g. lavapipe in WSL only supports one).
///
/// Drawing uses `tiny-skia` for filled rectangles and the gradient bar, and
/// `fontdue` with an embedded copy of Hack Regular for text.  See
/// `THIRD_PARTY_LICENSES.md` for attribution.
use fontdue::{Font, FontSettings};
use pixels::{Error, Pixels, SurfaceTexture};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tiny_skia::{Color, Paint, PixmapMut, Rect, Transform};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::core::{
    color_map::{ColorMap, ColorMapKeyFrame, ColorMapper},
    image_utils::{create_buffer, ImageSpecification, Renderable},
    interpolation::LinearInterpolator,
};

// Hack Regular, embedded at compile time.  License: MIT + Bitstream Vera.
// See THIRD_PARTY_LICENSES.md for full attribution.
static HACK_FONT_BYTES: &[u8] = include_bytes!("../../assets/Hack-Regular.ttf");

// ── Layout ────────────────────────────────────────────────────────────────────
//
//  ┌─────────────────────┬─────────────────────────┐
//  │   fractal preview   │      editor panel       │
//  │   PREVIEW_W × H     │      EDITOR_W × H       │
//  └─────────────────────┴─────────────────────────┘
//
const PREVIEW_W: u32 = 640;
const EDITOR_W: u32 = 860;
const TOTAL_W: u32 = PREVIEW_W + EDITOR_W;
const TOTAL_H: u32 = 480;

// Editor-panel local coordinates (origin = top-left of the editor pane).
const GRAD_X: u32 = 16;
const GRAD_W: u32 = EDITOR_W - GRAD_X * 2;
const GRAD_H: u32 = 44;
const GRAD_Y: u32 = 12;

// ── Palette ───────────────────────────────────────────────────────────────────
const BACKGROUND: [u8; 4] = [22, 22, 32, 255];
const WHITE: [u8; 3] = [255, 255, 255];
const DIM: [u8; 3] = [90, 90, 110];

// ── Text rendering ────────────────────────────────────────────────────────────

/// Blits a string into a flat RGBA frame using fontdue glyph coverage masks.
///
/// Coordinates are in terms of the full TOTAL_W × TOTAL_H frame.
/// `top` is the top of the em-square; the baseline is derived from the font's
/// ascent metric.
fn draw_text(frame: &mut [u8], font: &Font, text: &str, left: i32, top: i32, size: f32, color: [u8; 3]) {
    let ascent = font
        .horizontal_line_metrics(size)
        .map(|lm| lm.ascent.round() as i32)
        .unwrap_or(size.round() as i32);
    let baseline_y = top + ascent;

    let mut pen_x = left;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let coverage = bitmap[row * metrics.width + col];
                if coverage == 0 {
                    continue;
                }
                let px = pen_x + metrics.xmin + col as i32;
                let py = baseline_y - metrics.ymin - metrics.height as i32 + row as i32;
                if px < 0 || py < 0 || px >= TOTAL_W as i32 || py >= TOTAL_H as i32 {
                    continue;
                }
                let off = (py as u32 * TOTAL_W + px as u32) as usize * 4;
                if off + 3 >= frame.len() {
                    continue;
                }
                let a = coverage as u32;
                let inv = 255 - a;
                frame[off]     = ((color[0] as u32 * a + frame[off]     as u32 * inv) / 255) as u8;
                frame[off + 1] = ((color[1] as u32 * a + frame[off + 1] as u32 * inv) / 255) as u8;
                frame[off + 2] = ((color[2] as u32 * a + frame[off + 2] as u32 * inv) / 255) as u8;
            }
        }
        pen_x += metrics.advance_width.round() as i32;
    }
}

// ── Editor pane ───────────────────────────────────────────────────────────────

/// Renders the editor pane (right half of the window): background, gradient
/// bar, and hint text.  All coordinates are in the full frame; the pane starts
/// at x = PREVIEW_W.
fn draw_editor(frame: &mut [u8], keyframes: &[ColorMapKeyFrame], font: &Font) {
    let x0 = PREVIEW_W as f32;

    {
        let mut pixmap =
            PixmapMut::from_bytes(frame, TOTAL_W, TOTAL_H).expect("frame size mismatch");

        // Editor background.
        if let Some(rect) = Rect::from_xywh(x0, 0.0, EDITOR_W as f32, TOTAL_H as f32) {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(
                BACKGROUND[0],
                BACKGROUND[1],
                BACKGROUND[2],
                BACKGROUND[3],
            ));
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }

        // Gradient bar.
        if !keyframes.is_empty() {
            let color_map = ColorMap::new(keyframes, LinearInterpolator {});
            let mut paint = Paint {
                anti_alias: false,
                ..Paint::default()
            };
            for col in 0..GRAD_W {
                let t = col as f32 / (GRAD_W - 1).max(1) as f32;
                let rgb = color_map.compute_pixel(t);
                paint.set_color(Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255));
                if let Some(rect) = Rect::from_xywh(
                    x0 + (GRAD_X + col) as f32,
                    GRAD_Y as f32,
                    1.0,
                    GRAD_H as f32,
                ) {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
        }
    }

    // Text (absolute frame coordinates = PREVIEW_W + editor-local x).
    let tx = PREVIEW_W as i32 + 16;
    draw_text(frame, font, "Color Map Editor", tx, 80, 14.0, WHITE);
    draw_text(frame, font, "Coming soon: keyframe timeline and color picker", tx, 104, 12.0, DIM);
    draw_text(frame, font, "Q / Escape / close to quit", tx, 124, 12.0, DIM);
}

// ── Preview pane ──────────────────────────────────────────────────────────────

/// Copies a rendered fractal buffer (column-major `buf[x][y]`) into the left
/// pane of the full TOTAL_W × TOTAL_H frame.
///
/// The fractal is centred in the pane; any surrounding area is filled black so
/// that stale pixels from a previous frame don't bleed through.
fn draw_preview(frame: &mut [u8], buf: &[Vec<image::Rgb<u8>>], pw: u32, ph: u32) {
    let x_off = (PREVIEW_W.saturating_sub(pw)) / 2;
    let y_off = (TOTAL_H.saturating_sub(ph)) / 2;

    // Clear the entire preview pane to black first.
    for y in 0..TOTAL_H {
        for x in 0..PREVIEW_W {
            let off = (y * TOTAL_W + x) as usize * 4;
            frame[off]     = 0;
            frame[off + 1] = 0;
            frame[off + 2] = 0;
            frame[off + 3] = 255;
        }
    }

    // Blit the fractal centred within the pane.
    for x in 0..pw {
        for y in 0..ph {
            if (x as usize) < buf.len() && (y as usize) < buf[x as usize].len() {
                let rgb = buf[x as usize][y as usize];
                let off = ((y + y_off) * TOTAL_W + (x + x_off)) as usize * 4;
                if off + 3 < frame.len() {
                    frame[off]     = rgb[0];
                    frame[off + 1] = rgb[1];
                    frame[off + 2] = rgb[2];
                    frame[off + 3] = 255;
                }
            }
        }
    }
}

// ── Background render ─────────────────────────────────────────────────────────

/// Launches a background render thread if none is already running.
///
/// Returns `true` if a thread was started, `false` if the renderer was busy.
/// On completion the thread sets `ready` to `true` so the event loop can
/// schedule a redraw.
fn spawn_render<F: Renderable + Send + 'static>(
    renderer: Arc<Mutex<F>>,
    buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>>,
    busy: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
) -> bool {
    if busy.swap(true, Ordering::Acquire) {
        return false;
    }
    std::thread::spawn(move || {
        let mut buf = buffer.lock().unwrap();
        renderer.lock().unwrap().render_to_buffer(&mut buf);
        busy.store(false, Ordering::Release);
        ready.store(true, Ordering::Release);
    });
    true
}

/// Scales an `ImageSpecification` so that neither dimension exceeds the given
/// maximums, preserving aspect ratio and never upscaling.
fn scale_preview(spec: &ImageSpecification, max_w: u32, max_h: u32) -> ImageSpecification {
    let scale = (max_w as f64 / spec.resolution[0] as f64)
        .min(max_h as f64 / spec.resolution[1] as f64)
        .min(1.0);
    let pw = ((spec.resolution[0] as f64 * scale).round() as u32).max(1);
    let ph = ((spec.resolution[1] as f64 * scale).round() as u32).max(1);
    ImageSpecification {
        resolution: [pw, ph],
        center: spec.center,
        width: spec.width,
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Opens the color-map editor demo in a single window.
///
/// The left pane shows a fractal preview rendered in a background thread; the
/// right pane shows the color gradient and informational text.
/// Keyframe editing and save/load are added in later PRs.
///
/// # Controls
/// - **Q / Escape / close button** — quit
pub fn edit<F>(mut renderer: F, keyframes: Vec<ColorMapKeyFrame>) -> Result<(), Error>
where
    F: Renderable + Send + Sync + 'static,
    F::ReferenceCache: Send + 'static,
{
    let font = Font::from_bytes(HACK_FONT_BYTES, FontSettings::default())
        .expect("Failed to load embedded Hack font");

    let preview_spec = scale_preview(renderer.image_specification(), PREVIEW_W, TOTAL_H);
    let [pw, ph] = preview_spec.resolution;
    renderer.set_image_specification(preview_spec);

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Color Map Editor")
        .with_inner_size(LogicalSize::new(TOTAL_W as f64, TOTAL_H as f64))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    let mut pixels = {
        let sz = window.inner_size();
        Pixels::new(
            TOTAL_W,
            TOTAL_H,
            SurfaceTexture::new(sz.width, sz.height, &window),
        )?
    };

    let renderer = Arc::new(Mutex::new(renderer));
    let display_buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>> =
        Arc::new(Mutex::new(create_buffer(image::Rgb([0u8, 0, 0]), &[pw, ph])));
    let render_busy = Arc::new(AtomicBool::new(false));
    let render_ready = Arc::new(AtomicBool::new(false));

    spawn_render(
        renderer.clone(),
        display_buffer.clone(),
        render_busy.clone(),
        render_ready.clone(),
    );

    // Draw the editor pane immediately; the fractal pane will fill in once the
    // background render completes.
    draw_editor(pixels.frame_mut(), &keyframes, &font);
    pixels.render()?;

    event_loop.run(move |event, _, control_flow| {
        if let Event::NewEvents(StartCause::Init) = &event {
            *control_flow = ControlFlow::Wait;
        }

        if let Event::RedrawRequested(_) = &event {
            let buf = display_buffer.lock().unwrap();
            draw_preview(pixels.frame_mut(), &buf, pw, ph);
            drop(buf);
            draw_editor(pixels.frame_mut(), &keyframes, &font);
            if pixels.render().is_err() {
                eprintln!("ERROR: unable to render editor.");
                *control_flow = ControlFlow::Exit;
            }
        }

        if let Event::WindowEvent { event: window_event, .. } = &event {
            match window_event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    if matches!(
                        input.virtual_keycode,
                        Some(VirtualKeyCode::Escape) | Some(VirtualKeyCode::Q)
                    ) {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                _ => {}
            }
        }

        // Poll at 16 ms while a background render is running; sleep otherwise.
        if let Event::MainEventsCleared = &event {
            if render_ready.swap(false, Ordering::Relaxed) {
                window.request_redraw();
            }
            *control_flow = if render_busy.load(Ordering::Relaxed) {
                ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16))
            } else {
                ControlFlow::Wait
            };
        }
    })
}
