/// Interactive color-map editor.
///
/// This module provides the `edit` entry point that opens two windows side by
/// side: a live fractal preview and an editor panel.  In this first phase the
/// editor panel displays the color-gradient bar and basic keyboard shortcuts.
/// Keyframe timeline interaction and the HSV/RGB color picker are added in
/// subsequent pull requests.
use pixels::{Error, Pixels, SurfaceTexture};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::core::{
    color_map::{interpolate_keyframe_color, ColorMapEditable, ColorMapKeyFrame},
    image_utils::{create_buffer, ImageSpecification, Renderable},
};

// ── Editor window dimensions ──────────────────────────────────────────────────
const W: u32 = 900;
const H: u32 = 260; // Phase 1: gradient bar + hint text only

// ── Gradient bar layout ───────────────────────────────────────────────────────
const GRAD_X: u32 = 16;
const GRAD_W: u32 = W - GRAD_X * 2;
const GRAD_H: u32 = 44;
const GRAD_Y: u32 = 12;

// ── Palette ───────────────────────────────────────────────────────────────────

/// Dark navy background used for the editor panel.
const BACKGROUND: [u8; 4] = [22, 22, 32, 255];
/// Full white used for primary labels.
const WHITE: [u8; 4] = [255, 255, 255, 255];
/// Muted blue-grey used for secondary / hint text.
const DIM: [u8; 4] = [90, 90, 110, 255];

// ── Canvas ────────────────────────────────────────────────────────────────────

/// Thin wrapper around a raw RGBA pixel buffer that provides drawing primitives.
///
/// All coordinates are in pixels relative to the top-left corner of the buffer.
/// Writes outside the buffer bounds are silently ignored.
struct Canvas<'a> {
    frame: &'a mut [u8],
}

impl<'a> Canvas<'a> {
    /// Fills an axis-aligned rectangle with a solid color.
    fn fill_rect(&mut self, left: u32, top: u32, width: u32, height: u32, color: [u8; 4]) {
        for row in top..top + height {
            for col in left..left + width {
                let off = ((row * W + col) * 4) as usize;
                if off + 4 <= self.frame.len() {
                    self.frame[off..off + 4].copy_from_slice(&color);
                }
            }
        }
    }

    /// Draws a horizontal gradient strip by linearly interpolating across the
    /// given color-map keyframes from left to right.
    fn colormap_gradient(
        &mut self,
        left: u32,
        top: u32,
        width: u32,
        height: u32,
        keyframes: &[ColorMapKeyFrame],
    ) {
        if keyframes.is_empty() {
            return;
        }
        for col in 0..width {
            let t = col as f32 / (width - 1).max(1) as f32;
            let rgb = interpolate_keyframe_color(t, keyframes);
            let color = [rgb[0], rgb[1], rgb[2], 255];
            for row in top..top + height {
                let off = ((row * W + left + col) * 4) as usize;
                if off + 4 <= self.frame.len() {
                    self.frame[off..off + 4].copy_from_slice(&color);
                }
            }
        }
    }

    /// Renders a single 5×7 bitmap glyph at `(left, top)` scaled by `scale`.
    fn glyph(&mut self, ch: char, left: u32, top: u32, scale: u32, color: [u8; 4]) {
        let rows = glyph_bits(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5u32 {
                if bits & (0x10 >> col) != 0 {
                    self.fill_rect(
                        left + col * scale,
                        top + row as u32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
    }

    /// Renders a string of text starting at `(left, top)` using the bitmap font.
    fn text(&mut self, s: &str, left: u32, top: u32, scale: u32, color: [u8; 4]) {
        let mut x = left;
        for ch in s.chars() {
            self.glyph(ch, x, top, scale, color);
            x += 6 * scale;
        }
    }
}

// ── 5×7 bitmap font ───────────────────────────────────────────────────────────

/// Returns the 5×7 bitmap for character `c` as seven row bytes.
///
/// Each byte represents one row; bit 4 is the leftmost pixel.  Characters not
/// in the table render as a solid 5×7 block so that missing glyphs are visible.
fn glyph_bits(c: char) -> [u8; 7] {
    match c {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
        'a' => [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F],
        'c' => [0x00, 0x00, 0x0E, 0x10, 0x10, 0x11, 0x0E],
        'e' => [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E],
        'i' => [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E],
        'l' => [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'm' => [0x00, 0x00, 0x1A, 0x15, 0x15, 0x15, 0x15],
        'n' => [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11],
        'o' => [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E],
        'p' => [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10],
        'r' => [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10],
        's' => [0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x0E],
        't' => [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06],
        'u' => [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D],
        'x' => [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11],
        ':' => [0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x06],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        '/' => [0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00],
        ' ' => [0x00; 7],
        _ => [0x1F; 7],
    }
}

// ── Editor drawing ────────────────────────────────────────────────────────────

/// Renders the editor panel: background, gradient bar, and hint text.
fn draw_editor(frame: &mut [u8], keyframes: &[ColorMapKeyFrame]) {
    let mut canvas = Canvas { frame };
    canvas.fill_rect(0, 0, W, H, BACKGROUND);
    canvas.colormap_gradient(GRAD_X, GRAD_Y, GRAD_W, GRAD_H, keyframes);
    canvas.text("Color Map Editor", 16, 80, 1, WHITE);
    canvas.text(
        "Coming soon: keyframe timeline and color picker",
        16,
        100,
        1,
        DIM,
    );
    canvas.text("Q or Escape to quit   Space to save", 16, 120, 1, DIM);
}

// ── Preview drawing ───────────────────────────────────────────────────────────

/// Copies a rendered fractal buffer (column-major `buf[x][y]`) into the flat
/// RGBA pixel frame used by `pixels`.
fn draw_preview(frame: &mut [u8], buf: &[Vec<image::Rgb<u8>>], preview_width: u32) {
    for (flat, pixel) in frame.chunks_exact_mut(4).enumerate() {
        let x = flat as u32 % preview_width;
        let y = flat as u32 / preview_width;
        if (x as usize) < buf.len() && (y as usize) < buf[x as usize].len() {
            let rgb = buf[x as usize][y as usize];
            pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
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

/// Opens the interactive color-map editor for the given renderer.
///
/// Two windows appear: a fractal preview (rendered in a background thread) and
/// an editor panel showing the current color gradient.  The editor calls
/// `save_fn` with the current keyframes whenever the user presses Space or
/// closes a window.
///
/// # Controls
/// - **Q / Escape** — quit without saving
/// - **Space** — save keyframes via `save_fn`
/// - **Close button** — save and quit
pub fn edit<F, Save>(mut renderer: F, save_fn: Save) -> Result<(), Error>
where
    F: Renderable + ColorMapEditable + Send + Sync + 'static,
    F::ReferenceCache: Send + 'static,
    Save: Fn(&[ColorMapKeyFrame]) + 'static,
{
    let keyframes = renderer.get_keyframes();
    let preview_spec = scale_preview(renderer.image_specification(), 1280, 1280);
    let [pw, ph] = preview_spec.resolution;
    renderer.set_image_specification(preview_spec);

    let event_loop = EventLoop::new();

    let fractal_window = WindowBuilder::new()
        .with_title("Color Map Editor – Fractal Preview")
        .with_inner_size(LogicalSize::new(pw as f64, ph as f64))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();
    let fractal_wid = fractal_window.id();

    let editor_window = WindowBuilder::new()
        .with_title("Color Map Editor")
        .with_inner_size(LogicalSize::new(W as f64, H as f64))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();
    let editor_wid = editor_window.id();

    let mut fractal_pixels = {
        let sz = fractal_window.inner_size();
        Pixels::new(
            pw,
            ph,
            SurfaceTexture::new(sz.width, sz.height, &fractal_window),
        )?
    };
    let mut editor_pixels = {
        let sz = editor_window.inner_size();
        Pixels::new(
            W,
            H,
            SurfaceTexture::new(sz.width, sz.height, &editor_window),
        )?
    };

    let renderer = Arc::new(Mutex::new(renderer));
    let display_buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>> = Arc::new(Mutex::new(create_buffer(
        image::Rgb([0u8, 0, 0]),
        &[pw, ph],
    )));
    let render_busy = Arc::new(AtomicBool::new(false));
    let render_ready = Arc::new(AtomicBool::new(false));

    spawn_render(
        renderer.clone(),
        display_buffer.clone(),
        render_busy.clone(),
        render_ready.clone(),
    );

    draw_editor(editor_pixels.frame_mut(), &keyframes);
    editor_pixels.render()?;

    event_loop.run(move |event, _, control_flow| {
        // Startup: switch to event-driven scheduling so the thread sleeps
        // between events instead of spinning.  The `MainEventsCleared` handler
        // below re-enables polling while a background render is in progress.
        if let Event::NewEvents(StartCause::Init) = &event {
            *control_flow = ControlFlow::Wait;
        }

        if let Event::RedrawRequested(wid) = &event {
            if *wid == fractal_wid {
                let buf = display_buffer.lock().unwrap();
                draw_preview(fractal_pixels.frame_mut(), &buf, pw);
                drop(buf);
                if fractal_pixels.render().is_err() {
                    eprintln!("ERROR: unable to render fractal preview.");
                    *control_flow = ControlFlow::Exit;
                }
            } else if *wid == editor_wid {
                draw_editor(editor_pixels.frame_mut(), &keyframes);
                if editor_pixels.render().is_err() {
                    eprintln!("ERROR: unable to render editor.");
                    *control_flow = ControlFlow::Exit;
                }
            }
        }

        if let Event::WindowEvent {
            event: window_event,
            ..
        } = &event
        {
            match window_event {
                WindowEvent::CloseRequested => {
                    save_fn(&keyframes);
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    match input.virtual_keycode {
                        Some(VirtualKeyCode::Escape) | Some(VirtualKeyCode::Q) => {
                            *control_flow = ControlFlow::Exit;
                        }
                        Some(VirtualKeyCode::Space) => {
                            save_fn(&keyframes);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // `MainEventsCleared` fires once per event batch.  It is the sole place
        // where we update the `ControlFlow` schedule: poll at 16 ms while a
        // background render is running, then return to event-driven sleep.
        if let Event::MainEventsCleared = &event {
            if render_ready.swap(false, Ordering::Relaxed) {
                fractal_window.request_redraw();
            }
            *control_flow = if render_busy.load(Ordering::Relaxed) {
                ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16))
            } else {
                ControlFlow::Wait
            };
        }
    })
}
