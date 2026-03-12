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
    event::{ElementState, Event, MouseButton, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::core::{
    color_map::{ColorMap, ColorMapEditable, ColorMapKeyFrame, ColorMapper},
    image_utils::{create_buffer, ImageSpecification, Renderable},
    interpolation::LinearInterpolator,
};

// ── Editor window dimensions ────────────────────────────────────────────────
const W: u32 = 800;
const H: u32 = 460;

// ── Layout (y-coordinates) ───────────────────────────────────────────────────
const GRAD_Y: u32 = 16;
const GRAD_H: u32 = 52;

const TL_Y: u32 = 86;   // timeline top
const TL_H: u32 = 52;   // timeline height
const TL_CY: i32 = (TL_Y + TL_H / 2) as i32; // marker centre y

const PICK_Y: u32 = 158; // picker section top

const R_SY: u32 = PICK_Y + 8;
const G_SY: u32 = R_SY + 42;
const B_SY: u32 = G_SY + 42;
const SL_H: u32 = 28;

const SW_X: u32 = 16;   // swatch x
const SW_W: u32 = 52;   // swatch width
const SW_H: u32 = 3 * 42 - 14 + SL_H; // covers all three sliders

const SL_X: u32 = SW_X + SW_W + 12; // slider left edge
const SL_W: u32 = W - SL_X - 16;   // slider width

const BTN_Y: u32 = B_SY + SL_H + 20;
const BTN_SZ: u32 = 32;
const ADD_X: u32 = 16;
const REM_X: u32 = ADD_X + BTN_SZ + 10;

const MARKER_R: i32 = 11;
const MARKER_SEL_RING: i32 = 15; // outer ring radius when selected

const TL_X: u32 = 16;
const TL_W: u32 = W - 32;

// ── Colour palette ───────────────────────────────────────────────────────────
const BG:       [u8; 4] = [22,  22,  32,  255];
const BG_DARK:  [u8; 4] = [14,  14,  22,  255];
const BG_MID:   [u8; 4] = [38,  38,  52,  255];
const WHITE:    [u8; 4] = [255, 255, 255, 255];
const DIM:      [u8; 4] = [100, 100, 120, 255];
const ADD_COL:  [u8; 4] = [50,  160,  70, 255];
const REM_COL:  [u8; 4] = [180,  50,  50, 255];

// ── Drag state ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    None,
    Marker(usize),
    Red,
    Green,
    Blue,
}

// ── Editor state ─────────────────────────────────────────────────────────────
struct EditorState {
    keyframes: Vec<ColorMapKeyFrame>,
    selected: Option<usize>,
    drag: Drag,
    dirty: bool,         // keyframes changed, need fractal re-render
    save_requested: bool,
    quit_requested: bool,
    cursor: (f64, f64),
    mouse_down: bool,
}

impl EditorState {
    fn new(keyframes: Vec<ColorMapKeyFrame>) -> Self {
        Self {
            keyframes,
            selected: None,
            drag: Drag::None,
            dirty: true,
            save_requested: false,
            quit_requested: false,
            cursor: (0.0, 0.0),
            mouse_down: false,
        }
    }

    fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if self.mouse_down {
                    self.apply_drag(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                match state {
                    ElementState::Pressed => {
                        self.mouse_down = true;
                        self.on_mouse_down(self.cursor.0 as f32, self.cursor.1 as f32);
                    }
                    ElementState::Released => {
                        self.mouse_down = false;
                        self.drag = Drag::None;
                    }
                }
            }
            WindowEvent::KeyboardInput { input, .. } => {
                if input.state == ElementState::Pressed {
                    if let Some(key) = input.virtual_keycode {
                        self.on_key(key);
                    }
                }
            }
            WindowEvent::Focused(false) => {
                self.mouse_down = false;
                self.drag = Drag::None;
            }
            _ => {}
        }
    }

    fn on_key(&mut self, key: VirtualKeyCode) {
        match key {
            VirtualKeyCode::Escape => self.quit_requested = true,
            VirtualKeyCode::S => self.save_requested = true,
            VirtualKeyCode::N | VirtualKeyCode::Return => self.add_keyframe(),
            VirtualKeyCode::Delete | VirtualKeyCode::Back => self.remove_selected(),
            VirtualKeyCode::Left => self.select_adjacent(-1),
            VirtualKeyCode::Right => self.select_adjacent(1),
            _ => {}
        }
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        // Check + / - buttons
        if hit_rect(x, y, ADD_X, BTN_Y, BTN_SZ, BTN_SZ) {
            self.add_keyframe();
            return;
        }
        if hit_rect(x, y, REM_X, BTN_Y, BTN_SZ, BTN_SZ) {
            self.remove_selected();
            return;
        }

        // Check timeline markers
        for (i, kf) in self.keyframes.iter().enumerate() {
            let mx = query_to_x(kf.query);
            let dx = x as i32 - mx;
            let dy = y as i32 - TL_CY;
            if dx * dx + dy * dy <= (MARKER_SEL_RING + 4) * (MARKER_SEL_RING + 4) {
                self.selected = Some(i);
                self.drag = Drag::Marker(i);
                return;
            }
        }

        // Check RGB sliders (only when keyframe selected)
        if self.selected.is_some() {
            if hit_rect(x, y, SL_X, R_SY, SL_W, SL_H) {
                self.drag = Drag::Red;
                self.apply_slider(x, Drag::Red);
                return;
            }
            if hit_rect(x, y, SL_X, G_SY, SL_W, SL_H) {
                self.drag = Drag::Green;
                self.apply_slider(x, Drag::Green);
                return;
            }
            if hit_rect(x, y, SL_X, B_SY, SL_W, SL_H) {
                self.drag = Drag::Blue;
                self.apply_slider(x, Drag::Blue);
                return;
            }
        }

        // Click on empty timeline background — deselect
        if hit_rect(x, y, TL_X, TL_Y, TL_W, TL_H) {
            self.selected = None;
        }
    }

    fn apply_drag(&mut self, x: f32, _y: f32) {
        match self.drag {
            Drag::None => {}
            Drag::Marker(i) => self.drag_marker(i, x),
            d @ (Drag::Red | Drag::Green | Drag::Blue) => self.apply_slider(x, d),
        }
    }

    fn drag_marker(&mut self, i: usize, x: f32) {
        let n = self.keyframes.len();
        // First and last are fixed
        if i == 0 || i == n - 1 {
            return;
        }
        let lo = self.keyframes[i - 1].query + 0.001;
        let hi = self.keyframes[i + 1].query - 0.001;
        let q = x_to_query(x as i32).clamp(lo, hi);
        if (self.keyframes[i].query - q).abs() > 1e-5 {
            self.keyframes[i].query = q;
            self.dirty = true;
        }
    }

    fn apply_slider(&mut self, x: f32, component: Drag) {
        let Some(sel) = self.selected else { return };
        let val = x_to_component(x as i32);
        let rgb = &mut self.keyframes[sel].rgb_raw;
        let changed = match component {
            Drag::Red   => { let old = rgb[0]; rgb[0] = val; old != val }
            Drag::Green => { let old = rgb[1]; rgb[1] = val; old != val }
            Drag::Blue  => { let old = rgb[2]; rgb[2] = val; old != val }
            _ => false,
        };
        if changed {
            self.dirty = true;
        }
    }

    fn add_keyframe(&mut self) {
        // Insert midway between selected and next, or at 0.5 if nothing selected
        let (i, lo, hi) = if let Some(sel) = self.selected {
            let lo = self.keyframes[sel].query;
            let hi = if sel + 1 < self.keyframes.len() {
                self.keyframes[sel + 1].query
            } else {
                1.0
            };
            (sel + 1, lo, hi)
        } else {
            // find midpoint of largest gap
            let mut best = (0, 0.0_f32);
            for i in 0..self.keyframes.len() - 1 {
                let gap = self.keyframes[i + 1].query - self.keyframes[i].query;
                if gap > best.1 {
                    best = (i, gap);
                }
            }
            let i = best.0;
            (i + 1, self.keyframes[i].query, self.keyframes[i + 1].query)
        };
        if hi - lo < 0.002 {
            return;
        }
        let q = 0.5 * (lo + hi);
        // interpolate color between neighbors
        let c0 = self.keyframes[i - 1].rgb_raw;
        let c1 = self.keyframes[i].rgb_raw;
        let lerp = |a: u8, b: u8| (0.5 * (a as f32 + b as f32)).round() as u8;
        let rgb = [lerp(c0[0], c1[0]), lerp(c0[1], c1[1]), lerp(c0[2], c1[2])];
        self.keyframes.insert(i, ColorMapKeyFrame { query: q, rgb_raw: rgb });
        self.selected = Some(i);
        self.dirty = true;
    }

    fn remove_selected(&mut self) {
        let Some(sel) = self.selected else { return };
        let n = self.keyframes.len();
        if sel == 0 || sel == n - 1 || n <= 2 {
            return;
        }
        self.keyframes.remove(sel);
        self.selected = Some(sel.min(self.keyframes.len() - 2));
        self.dirty = true;
    }

    fn select_adjacent(&mut self, dir: i32) {
        let n = self.keyframes.len() as i32;
        let cur = self.selected.map(|i| i as i32).unwrap_or(0);
        self.selected = Some(((cur + dir).rem_euclid(n)) as usize);
    }
}

// ── Coordinate helpers ───────────────────────────────────────────────────────

fn query_to_x(q: f32) -> i32 {
    TL_X as i32 + (q * TL_W as f32).round() as i32
}

fn x_to_query(x: i32) -> f32 {
    ((x - TL_X as i32) as f32 / TL_W as f32).clamp(0.0, 1.0)
}

fn x_to_component(x: i32) -> u8 {
    ((x - SL_X as i32) as f32 / SL_W as f32 * 255.0)
        .clamp(0.0, 255.0)
        .round() as u8
}

fn component_x(val: u8) -> i32 {
    SL_X as i32 + (val as f32 / 255.0 * SL_W as f32).round() as i32
}

fn hit_rect(x: f32, y: f32, rx: u32, ry: u32, rw: u32, rh: u32) -> bool {
    x >= rx as f32 && x < (rx + rw) as f32 && y >= ry as f32 && y < (ry + rh) as f32
}

// ── Pixel drawing primitives ─────────────────────────────────────────────────

fn set_pixel(frame: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
        return;
    }
    let idx = ((y as u32 * W + x as u32) * 4) as usize;
    frame[idx..idx + 4].copy_from_slice(&color);
}

fn fill_rect(frame: &mut [u8], x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
    let x1 = (x + w).min(W);
    let y1 = (y + h).min(H);
    for py in y..y1 {
        for px in x..x1 {
            let idx = ((py * W + px) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&color);
        }
    }
}

fn draw_h_gradient(frame: &mut [u8], x: u32, y: u32, w: u32, h: u32, left: [u8; 3], right: [u8; 3]) {
    let x1 = (x + w).min(W);
    let y1 = (y + h).min(H);
    for px in x..x1 {
        let t = if w > 1 { (px - x) as f32 / (w - 1) as f32 } else { 0.0 };
        let lerp = |a: u8, b: u8| (a as f32 + t * (b as f32 - a as f32)).round() as u8;
        let c = [lerp(left[0], right[0]), lerp(left[1], right[1]), lerp(left[2], right[2]), 255];
        for py in y..y1 {
            let idx = ((py * W + px) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&c);
        }
    }
}

fn draw_colormap_gradient(
    frame: &mut [u8],
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color_map: &ColorMap<LinearInterpolator>,
) {
    let x1 = (x + w).min(W);
    let y1 = (y + h).min(H);
    for px in x..x1 {
        let t = if w > 1 { (px - x) as f32 / (w - 1) as f32 } else { 0.0 };
        let rgb = color_map.compute_pixel(t);
        let c = [rgb[0], rgb[1], rgb[2], 255];
        for py in y..y1 {
            let idx = ((py * W + px) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&c);
        }
    }
}

fn draw_circle(frame: &mut [u8], cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    let r2 = r * r;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r2 {
                set_pixel(frame, cx + dx, cy + dy, color);
            }
        }
    }
}

fn draw_ring(frame: &mut [u8], cx: i32, cy: i32, outer: i32, inner: i32, color: [u8; 4]) {
    let o2 = outer * outer;
    let i2 = inner * inner;
    for dy in -outer..=outer {
        for dx in -outer..=outer {
            let d2 = dx * dx + dy * dy;
            if d2 <= o2 && d2 > i2 {
                set_pixel(frame, cx + dx, cy + dy, color);
            }
        }
    }
}

/// Draws a 3-px-wide vertical thumb bar
fn draw_thumb(frame: &mut [u8], x: i32, y: u32, h: u32, color: [u8; 4]) {
    for dy in 0..h {
        for dx in -2i32..=2 {
            set_pixel(frame, x + dx, y as i32 + dy as i32, color);
        }
    }
}

fn draw_plus(frame: &mut [u8], cx: i32, cy: i32, arm: i32, color: [u8; 4]) {
    for d in -arm..=arm {
        set_pixel(frame, cx + d, cy, color);
        set_pixel(frame, cx, cy + d, color);
        set_pixel(frame, cx + d, cy - 1, color);
        set_pixel(frame, cx + d, cy + 1, color);
        set_pixel(frame, cx - 1, cy + d, color);
        set_pixel(frame, cx + 1, cy + d, color);
    }
}

fn draw_minus(frame: &mut [u8], cx: i32, cy: i32, arm: i32, color: [u8; 4]) {
    for d in -arm..=arm {
        set_pixel(frame, cx + d, cy - 1, color);
        set_pixel(frame, cx + d, cy, color);
        set_pixel(frame, cx + d, cy + 1, color);
    }
}

// ── Draw the entire editor frame ─────────────────────────────────────────────

fn draw_editor(frame: &mut [u8], state: &EditorState) {
    // Background
    fill_rect(frame, 0, 0, W, H, BG);

    let color_map = ColorMap::new(&state.keyframes, LinearInterpolator {});

    // ── Gradient bar ──────────────────────────────────────────────────────
    draw_colormap_gradient(frame, TL_X, GRAD_Y, TL_W, GRAD_H, &color_map);

    // ── Timeline ─────────────────────────────────────────────────────────
    fill_rect(frame, TL_X, TL_Y, TL_W, TL_H, BG_DARK);

    // Tick marks at 0.25 intervals
    for i in 0..=4u32 {
        let tx = TL_X as i32 + (i as f32 / 4.0 * TL_W as f32) as i32;
        for dy in 0..TL_H as i32 {
            set_pixel(frame, tx, TL_Y as i32 + dy, BG_MID);
        }
    }

    // Keyframe markers (back to front: unselected first)
    for (i, kf) in state.keyframes.iter().enumerate() {
        if state.selected == Some(i) {
            continue;
        }
        let mx = query_to_x(kf.query);
        let c = [kf.rgb_raw[0], kf.rgb_raw[1], kf.rgb_raw[2], 255];
        draw_circle(frame, mx, TL_CY, MARKER_R, c);
        draw_ring(frame, mx, TL_CY, MARKER_R, MARKER_R - 2, DIM);
    }
    // Draw selected marker on top with ring
    if let Some(sel) = state.selected {
        let kf = &state.keyframes[sel];
        let mx = query_to_x(kf.query);
        let c = [kf.rgb_raw[0], kf.rgb_raw[1], kf.rgb_raw[2], 255];
        draw_ring(frame, mx, TL_CY, MARKER_SEL_RING, MARKER_R, WHITE);
        draw_circle(frame, mx, TL_CY, MARKER_R, c);
    }

    // ── RGB sliders (only when something is selected) ─────────────────────
    if let Some(sel) = state.selected {
        let [r, g, b] = state.keyframes[sel].rgb_raw;

        // Color swatch
        fill_rect(frame, SW_X, R_SY, SW_W, SW_H, [r, g, b, 255]);
        draw_ring(frame, (SW_X + SW_W / 2) as i32, (R_SY + SW_H / 2) as i32,
                  (SW_W / 2) as i32, (SW_W / 2 - 2) as i32, DIM);

        // R slider
        draw_h_gradient(frame, SL_X, R_SY, SL_W, SL_H, [0, g, b], [255, g, b]);
        draw_thumb(frame, component_x(r), R_SY, SL_H, WHITE);

        // G slider
        draw_h_gradient(frame, SL_X, G_SY, SL_W, SL_H, [r, 0, b], [r, 255, b]);
        draw_thumb(frame, component_x(g), G_SY, SL_H, WHITE);

        // B slider
        draw_h_gradient(frame, SL_X, B_SY, SL_W, SL_H, [r, g, 0], [r, g, 255]);
        draw_thumb(frame, component_x(b), B_SY, SL_H, WHITE);
    } else {
        // Dim placeholder
        fill_rect(frame, SW_X, R_SY, W - SW_X * 2, SW_H, BG_MID);
        fill_rect(frame, TL_X, R_SY + SW_H / 2 - 1, TL_W, 2, DIM);
    }

    // ── Buttons ───────────────────────────────────────────────────────────
    fill_rect(frame, ADD_X, BTN_Y, BTN_SZ, BTN_SZ, ADD_COL);
    draw_plus(frame,
        (ADD_X + BTN_SZ / 2) as i32, (BTN_Y + BTN_SZ / 2) as i32,
        9, WHITE);

    fill_rect(frame, REM_X, BTN_Y, BTN_SZ, BTN_SZ, REM_COL);
    draw_minus(frame,
        (REM_X + BTN_SZ / 2) as i32, (BTN_Y + BTN_SZ / 2) as i32,
        9, WHITE);
}

// ── Draw the fractal preview ─────────────────────────────────────────────────

fn draw_preview(
    frame: &mut [u8],
    buffer: &[Vec<image::Rgb<u8>>],
    pw: u32,
    ph: u32,
) {
    debug_assert_eq!(frame.len(), (pw * ph * 4) as usize);
    for (flat, pixel) in frame.chunks_exact_mut(4).enumerate() {
        let x = (flat as u32) % pw;
        let y = (flat as u32) / pw;
        if (x as usize) < buffer.len() && (y as usize) < buffer[x as usize].len() {
            let rgb = buffer[x as usize][y as usize];
            pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
}

// ── Background render helper ─────────────────────────────────────────────────

fn spawn_render<F: Renderable + Send + 'static>(
    renderer: Arc<Mutex<F>>,
    buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>>,
    busy: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
) {
    if busy.swap(true, Ordering::Acquire) {
        return;
    }
    std::thread::spawn(move || {
        let mut buf = buffer.lock().unwrap();
        renderer.lock().unwrap().render_to_buffer(&mut buf);
        busy.store(false, Ordering::Release);
        ready.store(true, Ordering::Release);
    });
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Scale an `ImageSpecification` so it fits within `max_w × max_h` pixels.
fn scale_preview(spec: &ImageSpecification, max_w: u32, max_h: u32) -> ImageSpecification {
    let sx = max_w as f64 / spec.resolution[0] as f64;
    let sy = max_h as f64 / spec.resolution[1] as f64;
    let scale = sx.min(sy).min(1.0);
    let pw = ((spec.resolution[0] as f64 * scale).round() as u32).max(1);
    let ph = ((spec.resolution[1] as f64 * scale).round() as u32).max(1);
    ImageSpecification {
        resolution: [pw, ph],
        center: spec.center,
        width: spec.width,
    }
}

/// Open the two-window color-map editor.
///
/// * `renderer`   – fractal renderer (implements both `Renderable` and `ColorMapEditable`)
/// * `save_fn`    – called with the final keyframes on save ('S') or window close
pub fn edit<F, Save>(renderer: F, save_fn: Save) -> Result<(), Error>
where
    F: Renderable + ColorMapEditable + Send + Sync + 'static,
    Save: Fn(&[ColorMapKeyFrame]) + 'static,
{
    let initial_keyframes = renderer.get_keyframes();
    let preview_spec = scale_preview(renderer.image_specification(), 960, 540);
    let [pw, ph] = preview_spec.resolution;

    eprintln!(
        "\nColor Map Editor Controls:\n\
         \x20 • Click/drag markers on the timeline to reposition keyframes\n\
         \x20 • Click/drag R/G/B sliders to change the selected keyframe's color\n\
         \x20 • Left/Right arrow keys to change selected keyframe\n\
         \x20 • N or Enter  – add keyframe at midpoint\n\
         \x20 • Delete/Backspace – remove selected keyframe\n\
         \x20 • S            – save params back to file\n\
         \x20 • Escape or close either window – save and quit\n"
    );

    let event_loop = EventLoop::new();

    let fractal_window = WindowBuilder::new()
        .with_title("Color Map Editor – Fractal Preview")
        .with_inner_size(LogicalSize::new(pw as f64, ph as f64))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    let editor_window = WindowBuilder::new()
        .with_title("Color Map Editor – Controls")
        .with_inner_size(LogicalSize::new(W as f64, H as f64))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    let fractal_wid = fractal_window.id();
    let editor_wid = editor_window.id();

    let mut fractal_pixels = {
        let sz = fractal_window.inner_size();
        Pixels::new(pw, ph, SurfaceTexture::new(sz.width, sz.height, &fractal_window))?
    };
    let mut editor_pixels = {
        let sz = editor_window.inner_size();
        Pixels::new(W, H, SurfaceTexture::new(sz.width, sz.height, &editor_window))?
    };

    // Set up the renderer with the preview resolution (initialises CDF/histogram)
    let mut renderer = renderer;
    renderer.set_image_specification(preview_spec);

    let renderer = Arc::new(Mutex::new(renderer));
    let display_buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>> =
        Arc::new(Mutex::new(create_buffer(image::Rgb([0u8, 0, 0]), &[pw, ph])));
    let render_busy = Arc::new(AtomicBool::new(false));
    let render_ready = Arc::new(AtomicBool::new(false));

    let mut state = EditorState::new(initial_keyframes);

    // Kick off first render
    spawn_render(
        renderer.clone(),
        display_buffer.clone(),
        render_busy.clone(),
        render_ready.clone(),
    );

    event_loop.run(move |event, _, control_flow| {
        if let Event::NewEvents(StartCause::Init) = event {
            *control_flow = ControlFlow::Wait;
        }

        // ── Window events ─────────────────────────────────────────────────
        if let Event::WindowEvent { window_id, ref event } = event {
            match event {
                WindowEvent::CloseRequested => {
                    save_fn(&state.keyframes);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                WindowEvent::Resized(sz) if window_id == fractal_wid => {
                    fractal_pixels.resize_surface(sz.width, sz.height).ok();
                }
                WindowEvent::Resized(sz) if window_id == editor_wid => {
                    editor_pixels.resize_surface(sz.width, sz.height).ok();
                }
                _ if window_id == editor_wid => {
                    state.handle_window_event(event);
                }
                _ => {}
            }
        }

        // ── Redraw requests ───────────────────────────────────────────────
        if let Event::RedrawRequested(wid) = event {
            if wid == fractal_wid {
                let buf = display_buffer.lock().unwrap();
                draw_preview(fractal_pixels.frame_mut(), &buf, pw, ph);
                fractal_pixels.render().ok();
            } else if wid == editor_wid {
                draw_editor(editor_pixels.frame_mut(), &state);
                editor_pixels.render().ok();
            }
        }

        // ── Main update tick ──────────────────────────────────────────────
        if let Event::MainEventsCleared = event {
            if state.quit_requested {
                save_fn(&state.keyframes);
                *control_flow = ControlFlow::Exit;
                return;
            }

            if state.save_requested {
                save_fn(&state.keyframes);
                state.save_requested = false;
            }

            let mut need_editor_redraw = false;

            if state.dirty {
                state.dirty = false;
                need_editor_redraw = true;
                let kf = state.keyframes.clone();
                renderer.lock().unwrap().set_keyframes(kf);
                spawn_render(
                    renderer.clone(),
                    display_buffer.clone(),
                    render_busy.clone(),
                    render_ready.clone(),
                );
            }

            if render_ready.swap(false, Ordering::AcqRel) {
                fractal_window.request_redraw();
            }

            if need_editor_redraw {
                editor_window.request_redraw();
            }

            let busy = render_busy.load(Ordering::Acquire) || state.mouse_down;
            *control_flow = if busy {
                ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16))
            } else {
                ControlFlow::Wait
            };
        }
    });
}
