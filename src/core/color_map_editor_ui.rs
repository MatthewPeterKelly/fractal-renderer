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
    color_map::{ColorMapEditable, ColorMapKeyFrame},
    image_utils::{create_buffer, ImageSpecification, Renderable},
};

// ── Editor window dimensions ──────────────────────────────────────────────────
const W: u32 = 900;
const H: u32 = 260; // Phase 1: header + gradient bar only

// ── Gradient bar layout ───────────────────────────────────────────────────────
const GRAD_X: u32 = 16;
const GRAD_W: u32 = W - GRAD_X * 2;
const GRAD_H: u32 = 44;
const GRAD_Y: u32 = 12;

// ── Palette ───────────────────────────────────────────────────────────────────
const BG: [u8; 4] = [22, 22, 32, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const DIM: [u8; 4] = [90, 90, 110, 255];

// ── Minimal canvas ────────────────────────────────────────────────────────────

struct Canvas<'a> {
    frame: &'a mut [u8],
}

impl<'a> Canvas<'a> {
    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, c: [u8; 4]) {
        for row in y..y + h {
            for col in x..x + w {
                let off = ((row * W + col) * 4) as usize;
                if off + 4 <= self.frame.len() {
                    self.frame[off..off + 4].copy_from_slice(&c);
                }
            }
        }
    }

    /// Draw a horizontal gradient strip by linearly interpolating the keyframes.
    fn colormap_gradient(&mut self, x: u32, y: u32, w: u32, h: u32, kf: &[ColorMapKeyFrame]) {
        if kf.is_empty() {
            return;
        }
        for col in 0..w {
            let t = col as f32 / (w - 1).max(1) as f32;
            let rgb = interp_keyframes(t, kf);
            let c = [rgb[0], rgb[1], rgb[2], 255];
            for row in y..y + h {
                let off = ((row * W + x + col) * 4) as usize;
                if off + 4 <= self.frame.len() {
                    self.frame[off..off + 4].copy_from_slice(&c);
                }
            }
        }
    }

    fn glyph(&mut self, ch: char, x: u32, y: u32, scale: u32, c: [u8; 4]) {
        let rows = glyph_bits(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5u32 {
                if bits & (0x10 >> col) != 0 {
                    self.fill_rect(x + col * scale, y + row as u32 * scale, scale, scale, c);
                }
            }
        }
    }

    fn text(&mut self, s: &str, x: u32, y: u32, scale: u32, c: [u8; 4]) {
        let mut cx = x;
        for ch in s.chars() {
            self.glyph(ch, cx, y, scale, c);
            cx += 6 * scale;
        }
    }
}

// ── Color helpers ─────────────────────────────────────────────────────────────

fn interp_keyframes(t: f32, kf: &[ColorMapKeyFrame]) -> [u8; 3] {
    if kf.len() == 1 {
        return kf[0].rgb_raw;
    }
    if t <= kf[0].query {
        return kf[0].rgb_raw;
    }
    let last = kf.last().unwrap();
    if t >= last.query {
        return last.rgb_raw;
    }
    for i in 1..kf.len() {
        let a = &kf[i - 1];
        let b = &kf[i];
        if t <= b.query {
            let s = (t - a.query) / (b.query - a.query);
            return [
                lerp_u8(a.rgb_raw[0], b.rgb_raw[0], s),
                lerp_u8(a.rgb_raw[1], b.rgb_raw[1], s),
                lerp_u8(a.rgb_raw[2], b.rgb_raw[2], s),
            ];
        }
    }
    last.rgb_raw
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + t * (b as f32 - a as f32)).round() as u8
}

// ── 5×7 bitmap font ───────────────────────────────────────────────────────────

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
        _   => [0x1F; 7],
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw_editor(frame: &mut [u8], keyframes: &[ColorMapKeyFrame]) {
    let mut c = Canvas { frame };
    c.fill_rect(0, 0, W, H, BG);
    c.colormap_gradient(GRAD_X, GRAD_Y, GRAD_W, GRAD_H, keyframes);
    c.text("Color Map Editor", 16, 80, 1, WHITE);
    c.text("Coming soon: keyframe timeline and color picker", 16, 100, 1, DIM);
    c.text("Q or Escape to quit   Space to save", 16, 120, 1, DIM);
}

fn draw_preview(frame: &mut [u8], buf: &[Vec<image::Rgb<u8>>], pw: u32) {
    for (flat, pixel) in frame.chunks_exact_mut(4).enumerate() {
        let x = flat as u32 % pw;
        let y = flat as u32 / pw;
        if (x as usize) < buf.len() && (y as usize) < buf[x as usize].len() {
            let rgb = buf[x as usize][y as usize];
            pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
}

// ── Background render ─────────────────────────────────────────────────────────

fn spawn_render<F: Renderable + Send + 'static>(
    renderer: Arc<Mutex<F>>,
    buffer:   Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>>,
    busy:     Arc<AtomicBool>,
    ready:    Arc<AtomicBool>,
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

fn scale_preview(spec: &ImageSpecification, max_w: u32, max_h: u32) -> ImageSpecification {
    let scale = (max_w as f64 / spec.resolution[0] as f64)
        .min(max_h as f64 / spec.resolution[1] as f64)
        .min(1.0);
    let pw = ((spec.resolution[0] as f64 * scale).round() as u32).max(1);
    let ph = ((spec.resolution[1] as f64 * scale).round() as u32).max(1);
    ImageSpecification { resolution: [pw, ph], center: spec.center, width: spec.width }
}

// ── Public entry point ────────────────────────────────────────────────────────

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
        Pixels::new(pw, ph, SurfaceTexture::new(sz.width, sz.height, &fractal_window))?
    };
    let mut editor_pixels = {
        let sz = editor_window.inner_size();
        Pixels::new(W, H, SurfaceTexture::new(sz.width, sz.height, &editor_window))?
    };

    let renderer = Arc::new(Mutex::new(renderer));
    let display_buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>> =
        Arc::new(Mutex::new(create_buffer(image::Rgb([0u8, 0, 0]), &[pw, ph])));
    let render_busy  = Arc::new(AtomicBool::new(false));
    let render_ready = Arc::new(AtomicBool::new(false));

    spawn_render(renderer.clone(), display_buffer.clone(), render_busy.clone(), render_ready.clone());

    draw_editor(editor_pixels.frame_mut(), &keyframes);
    editor_pixels.render()?;

    let mut quit = false;

    event_loop.run(move |event, _, control_flow| {
        if quit {
            *control_flow = ControlFlow::Exit;
            return;
        }
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16));

        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if render_ready.swap(false, Ordering::Relaxed) {
                    fractal_window.request_redraw();
                }
            }

            Event::RedrawRequested(wid) if wid == fractal_wid => {
                let buf = display_buffer.lock().unwrap();
                draw_preview(fractal_pixels.frame_mut(), &buf, pw);
                drop(buf);
                fractal_pixels.render().unwrap();
            }

            Event::RedrawRequested(wid) if wid == editor_wid => {
                draw_editor(editor_pixels.frame_mut(), &keyframes);
                editor_pixels.render().unwrap();
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    save_fn(&keyframes);
                    quit = true;
                }
                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    match input.virtual_keycode {
                        Some(VirtualKeyCode::Escape) | Some(VirtualKeyCode::Q) => {
                            quit = true;
                        }
                        Some(VirtualKeyCode::Space) => {
                            save_fn(&keyframes);
                        }
                        _ => {}
                    }
                }
                _ => {}
            },

            _ => {}
        }
    })
}
