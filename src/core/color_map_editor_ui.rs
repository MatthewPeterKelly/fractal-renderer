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
    event::{ElementState, Event, ModifiersState, MouseButton, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::core::{
    color_map::{ColorMap, ColorMapEditable, ColorMapKeyFrame, ColorMapper},
    image_utils::{create_buffer, ImageSpecification, Renderable},
    interpolation::LinearInterpolator,
};

// ── Window dimensions ────────────────────────────────────────────────────────
const W: u32 = 800;
const H: u32 = 420;

// ── Layout ───────────────────────────────────────────────────────────────────
const TL_X: u32 = 16;
const TL_W: u32 = W - 32;

const GRAD_Y: u32 = 14;
const GRAD_H: u32 = 44;

const TL_Y:  u32 = 68;
const TL_H:  u32 = 80;
// Markers stack upward from TL_BASE_Y; row 0 = bottom row.
const TL_BASE_Y:    i32 = (TL_Y + TL_H) as i32 - MARKER_R - 4;
const MARKER_R:     i32 = 9;
const MARKER_SEL:   i32 = 13;  // selection ring outer radius
const MARKER_STRIDE: i32 = 2 * MARKER_R + 4;

// Color-picker section
const PICK_Y:  u32 = 158;
const SV_X:    u32 = TL_X;
const SV_Y:    u32 = PICK_Y;
const SV_SIZE: u32 = 200;
const HS_X:    u32 = SV_X + SV_SIZE + 8;   // hue strip x
const HS_W:    u32 = 22;                    // hue strip width
const HS_H:    u32 = SV_SIZE;
const SWATCH_X: u32 = HS_X + HS_W + 8;
const SWATCH_W: u32 = 58;
const SWATCH_H: u32 = 58;

// Buttons
const BTN_Y:  u32 = PICK_Y + SV_SIZE + 12;
const BTN_SZ: u32 = 32;
const ADD_X:  u32 = TL_X;
const REM_X:  u32 = ADD_X + BTN_SZ + 10;

// ── Palette ──────────────────────────────────────────────────────────────────
const BG:      [u8; 4] = [22,  22,  32,  255];
const BG_DARK: [u8; 4] = [14,  14,  22,  255];
const BG_MID:  [u8; 4] = [38,  38,  52,  255];
const WHITE:   [u8; 4] = [255, 255, 255, 255];
const DIM:     [u8; 4] = [80,  80,  100, 255];
const ADD_COL: [u8; 4] = [50,  160,  70, 255];
const REM_COL: [u8; 4] = [180,  50,  50, 255];

// ── HSV conversion ───────────────────────────────────────────────────────────

/// Returns H ∈ [0,360), S ∈ [0,1], V ∈ [0,1].
fn rgb_to_hsv(rgb: [u8; 3]) -> [f32; 3] {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 1e-6 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max < 1e-6 { 0.0 } else { d / max };
    [h, s, max]
}

fn hsv_to_rgb(hsv: [f32; 3]) -> [u8; 3] {
    let [h, s, v] = [hsv[0], hsv[1].clamp(0.0, 1.0), hsv[2].clamp(0.0, 1.0)];
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

fn hue_to_rgb(h: f32) -> [u8; 3] {
    hsv_to_rgb([h, 1.0, 1.0])
}

// ── Stagger helpers ──────────────────────────────────────────────────────────

/// For each keyframe, returns its stagger row (0 = bottom/foreground).
/// Overlapping markers (within 2*MARKER_R px) are placed in different rows.
fn compute_stagger(keyframes: &[ColorMapKeyFrame]) -> Vec<i32> {
    let xs: Vec<i32> = keyframes.iter().map(|kf| query_to_x(kf.query)).collect();
    let mut rows = vec![0i32; keyframes.len()];
    for i in 0..keyframes.len() {
        let occupied: Vec<i32> = (0..i)
            .filter(|&j| (xs[i] - xs[j]).abs() < 2 * MARKER_R + 2)
            .map(|j| rows[j])
            .collect();
        rows[i] = (0..).find(|r| !occupied.contains(r)).unwrap();
    }
    rows
}

fn stagger_cy(row: i32) -> i32 {
    TL_BASE_Y - row * MARKER_STRIDE
}

// ── Coordinate helpers ───────────────────────────────────────────────────────

fn query_to_x(q: f32) -> i32 {
    TL_X as i32 + (q * TL_W as f32).round() as i32
}

fn x_to_query(x: i32) -> f32 {
    ((x - TL_X as i32) as f32 / TL_W as f32).clamp(0.0, 1.0)
}

fn hit_rect(x: f32, y: f32, rx: u32, ry: u32, rw: u32, rh: u32) -> bool {
    x >= rx as f32 && x < (rx + rw) as f32 && y >= ry as f32 && y < (ry + rh) as f32
}

// ── Drag state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    None,
    Marker(usize),
    SVSquare,
    HueStrip,
}

// ── Editor state ─────────────────────────────────────────────────────────────

struct EditorState {
    keyframes: Vec<ColorMapKeyFrame>,
    selected: Option<usize>,
    /// HSV for the selected keyframe; kept in sync to avoid rounding artifacts
    /// while dragging within the picker.
    hsv: [f32; 3],
    drag: Drag,
    /// Keyframes changed → update renderer + editor.
    keyframes_dirty: bool,
    /// Only the view changed (e.g. selection) → redraw editor only.
    view_dirty: bool,
    save_requested: bool,
    quit_requested: bool,
    cursor: (f64, f64),
    mouse_down: bool,
    modifiers: ModifiersState,
}

impl EditorState {
    fn new(keyframes: Vec<ColorMapKeyFrame>) -> Self {
        Self {
            keyframes,
            selected: None,
            hsv: [0.0, 0.0, 1.0],
            drag: Drag::None,
            keyframes_dirty: true,
            view_dirty: false,
            save_requested: false,
            quit_requested: false,
            cursor: (0.0, 0.0),
            mouse_down: false,
            modifiers: ModifiersState::default(),
        }
    }

    fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(m) => self.modifiers = *m,
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if self.mouse_down {
                    self.apply_drag(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => {
                    self.mouse_down = true;
                    self.on_mouse_down(self.cursor.0 as f32, self.cursor.1 as f32);
                }
                ElementState::Released => {
                    self.mouse_down = false;
                    self.drag = Drag::None;
                }
            },
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
            // Shift+arrow: nudge query position; plain arrow: cycle selection.
            VirtualKeyCode::Left if self.modifiers.shift() => self.nudge_query(-1),
            VirtualKeyCode::Right if self.modifiers.shift() => self.nudge_query(1),
            VirtualKeyCode::Left => self.cycle_selected(-1),
            VirtualKeyCode::Right => self.cycle_selected(1),
            _ => {}
        }
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        // Buttons
        if hit_rect(x, y, ADD_X, BTN_Y, BTN_SZ, BTN_SZ) {
            self.add_keyframe();
            return;
        }
        if hit_rect(x, y, REM_X, BTN_Y, BTN_SZ, BTN_SZ) {
            self.remove_selected();
            return;
        }

        // Timeline markers (check all; prefer the selected one on tie)
        let stagger = compute_stagger(&self.keyframes);
        let mut best: Option<(usize, i32)> = None;
        for (i, kf) in self.keyframes.iter().enumerate() {
            let mx = query_to_x(kf.query);
            let my = stagger_cy(stagger[i]);
            let dx = x as i32 - mx;
            let dy = y as i32 - my;
            let d2 = dx * dx + dy * dy;
            let hit_r = MARKER_SEL + 4;
            if d2 <= hit_r * hit_r {
                let is_better = best.map_or(true, |(prev_i, prev_d2)| {
                    // prefer currently selected; otherwise prefer closest
                    let sel_bonus = |idx: usize| if self.selected == Some(idx) { 0 } else { 1 };
                    (sel_bonus(i), d2) < (sel_bonus(prev_i), prev_d2)
                });
                if is_better {
                    best = Some((i, d2));
                }
            }
        }
        if let Some((i, _)) = best {
            self.set_selected(Some(i));
            self.drag = Drag::Marker(i);
            return;
        }

        // SV square (only when keyframe selected)
        if self.selected.is_some() && hit_rect(x, y, SV_X, SV_Y, SV_SIZE, SV_SIZE) {
            self.drag = Drag::SVSquare;
            self.apply_sv_drag(x, y);
            return;
        }

        // H strip
        if self.selected.is_some() && hit_rect(x, y, HS_X, SV_Y, HS_W, HS_H) {
            self.drag = Drag::HueStrip;
            self.apply_h_drag(y);
            return;
        }

        // Empty timeline → deselect
        if hit_rect(x, y, TL_X, TL_Y, TL_W, TL_H) {
            self.set_selected(None);
        }
    }

    fn apply_drag(&mut self, x: f32, y: f32) {
        match self.drag {
            Drag::None => {}
            Drag::Marker(i) => self.drag_marker(i, x),
            Drag::SVSquare => self.apply_sv_drag(x, y),
            Drag::HueStrip => self.apply_h_drag(y),
        }
    }

    fn drag_marker(&mut self, i: usize, x: f32) {
        let n = self.keyframes.len();
        if i == 0 || i == n - 1 {
            return;
        }
        let lo = self.keyframes[i - 1].query + 0.001;
        let hi = self.keyframes[i + 1].query - 0.001;
        let q = x_to_query(x as i32).clamp(lo, hi);
        if (self.keyframes[i].query - q).abs() > 1e-6 {
            self.keyframes[i].query = q;
            self.keyframes_dirty = true;
        }
    }

    fn apply_sv_drag(&mut self, x: f32, y: f32) {
        let s = ((x - SV_X as f32) / SV_SIZE as f32).clamp(0.0, 1.0);
        let v = 1.0 - ((y - SV_Y as f32) / SV_SIZE as f32).clamp(0.0, 1.0);
        self.hsv[1] = s;
        self.hsv[2] = v;
        self.flush_hsv();
    }

    fn apply_h_drag(&mut self, y: f32) {
        let h = ((y - SV_Y as f32) / HS_H as f32).clamp(0.0, 1.0) * 360.0;
        self.hsv[0] = h;
        self.flush_hsv();
    }

    /// Write current `self.hsv` back to the selected keyframe's rgb_raw.
    fn flush_hsv(&mut self) {
        let Some(sel) = self.selected else { return };
        let new_rgb = hsv_to_rgb(self.hsv);
        if self.keyframes[sel].rgb_raw != new_rgb {
            self.keyframes[sel].rgb_raw = new_rgb;
            self.keyframes_dirty = true;
        }
    }

    fn set_selected(&mut self, idx: Option<usize>) {
        if self.selected != idx {
            self.selected = idx;
            self.sync_hsv();
            self.view_dirty = true;
        }
    }

    /// Refresh `self.hsv` from the currently selected keyframe.
    fn sync_hsv(&mut self) {
        if let Some(sel) = self.selected {
            self.hsv = rgb_to_hsv(self.keyframes[sel].rgb_raw);
        }
    }

    fn cycle_selected(&mut self, dir: i32) {
        let n = self.keyframes.len() as i32;
        let cur = self.selected.map(|i| i as i32).unwrap_or(-1);
        let next = ((cur + dir).rem_euclid(n)) as usize;
        self.set_selected(Some(next));
    }

    /// Move the selected keyframe's query value 10 % of the way toward its
    /// left (dir = -1) or right (dir = +1) neighbour. This is idempotent and
    /// asymptotically convergent: it can never cross a neighbour.
    fn nudge_query(&mut self, dir: i32) {
        let Some(sel) = self.selected else { return };
        let n = self.keyframes.len();
        if sel == 0 || sel == n - 1 {
            return;
        }
        let cur = self.keyframes[sel].query;
        let lo = self.keyframes[sel - 1].query;
        let hi = self.keyframes[sel + 1].query;
        let new_q = if dir < 0 {
            cur - 0.1 * (cur - lo) // 10 % toward left neighbour
        } else {
            cur + 0.1 * (hi - cur) // 10 % toward right neighbour
        };
        let new_q = new_q.clamp(lo + 1e-5, hi - 1e-5);
        if (new_q - cur).abs() > 1e-7 {
            self.keyframes[sel].query = new_q;
            self.keyframes_dirty = true;
        }
    }

    fn add_keyframe(&mut self) {
        let (insert_at, lo, hi) = if let Some(sel) = self.selected {
            let lo = self.keyframes[sel].query;
            let hi = self.keyframes.get(sel + 1).map_or(1.0, |k| k.query);
            (sel + 1, lo, hi)
        } else {
            // Largest gap
            let (best_i, _) = (0..self.keyframes.len() - 1)
                .map(|i| (i, self.keyframes[i + 1].query - self.keyframes[i].query))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap_or((0, 0.0));
            (best_i + 1, self.keyframes[best_i].query, self.keyframes[best_i + 1].query)
        };
        if hi - lo < 0.002 {
            return;
        }
        let q = 0.5 * (lo + hi);
        let c0 = self.keyframes[insert_at - 1].rgb_raw;
        let c1 = self.keyframes[insert_at].rgb_raw;
        let lerp = |a: u8, b: u8| (0.5 * (a as f32 + b as f32)).round() as u8;
        let rgb = [lerp(c0[0], c1[0]), lerp(c0[1], c1[1]), lerp(c0[2], c1[2])];
        self.keyframes.insert(insert_at, ColorMapKeyFrame { query: q, rgb_raw: rgb });
        self.set_selected(Some(insert_at));
        self.keyframes_dirty = true;
    }

    fn remove_selected(&mut self) {
        let Some(sel) = self.selected else { return };
        let n = self.keyframes.len();
        if sel == 0 || sel == n - 1 || n <= 2 {
            return;
        }
        self.keyframes.remove(sel);
        let new_sel = sel.min(self.keyframes.len() - 2);
        self.set_selected(Some(new_sel));
        self.keyframes_dirty = true;
    }
}

// ── Drawing primitives ───────────────────────────────────────────────────────

fn set_pixel(frame: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
        return;
    }
    let idx = ((y as u32 * W + x as u32) * 4) as usize;
    frame[idx..idx + 4].copy_from_slice(&color);
}

fn fill_rect(frame: &mut [u8], x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
    for py in y..(y + h).min(H) {
        for px in x..(x + w).min(W) {
            let idx = ((py * W + px) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&color);
        }
    }
}

fn draw_colormap_gradient(frame: &mut [u8], x: u32, y: u32, w: u32, h: u32,
                           color_map: &ColorMap<LinearInterpolator>) {
    for px in x..(x + w).min(W) {
        let t = if w > 1 { (px - x) as f32 / (w - 1) as f32 } else { 0.0 };
        let rgb = color_map.compute_pixel(t);
        let c = [rgb[0], rgb[1], rgb[2], 255];
        for py in y..(y + h).min(H) {
            let idx = ((py * W + px) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&c);
        }
    }
}

fn draw_sv_square(frame: &mut [u8], x: u32, y: u32, size: u32, hue: f32) {
    for py in 0..size {
        let v = 1.0 - py as f32 / (size - 1) as f32;
        for px in 0..size {
            let s = px as f32 / (size - 1) as f32;
            let rgb = hsv_to_rgb([hue, s, v]);
            let idx = (((y + py) * W + (x + px)) * 4) as usize;
            if idx + 3 < frame.len() {
                frame[idx]     = rgb[0];
                frame[idx + 1] = rgb[1];
                frame[idx + 2] = rgb[2];
                frame[idx + 3] = 255;
            }
        }
    }
}

fn draw_h_strip(frame: &mut [u8], x: u32, y: u32, w: u32, h: u32) {
    for py in 0..h {
        let hue = py as f32 / (h - 1) as f32 * 360.0;
        let rgb = hue_to_rgb(hue);
        let c = [rgb[0], rgb[1], rgb[2], 255];
        for px in x..(x + w).min(W) {
            let idx = (((y + py) * W + px) * 4) as usize;
            if idx + 3 < frame.len() {
                frame[idx..idx + 4].copy_from_slice(&c);
            }
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

/// Small right-pointing arrow (▶) as a hue-strip marker.
fn draw_arrow_right(frame: &mut [u8], tip_x: i32, tip_y: i32, size: i32, color: [u8; 4]) {
    for dy in -size..=size {
        for dx in 0..=(size - dy.abs()) {
            set_pixel(frame, tip_x - dx, tip_y + dy, color);
        }
    }
}

/// Crosshair ring used to mark the current position in the SV square.
fn draw_crosshair(frame: &mut [u8], cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    draw_ring(frame, cx, cy, r, r - 2, color);
    // small cross
    for d in -(r + 3)..=(r + 3) {
        if d.abs() > r {
            set_pixel(frame, cx + d, cy, color);
            set_pixel(frame, cx, cy + d, color);
        }
    }
}

fn draw_plus(frame: &mut [u8], cx: i32, cy: i32, arm: i32, color: [u8; 4]) {
    for d in -arm..=arm {
        for t in -1i32..=1 {
            set_pixel(frame, cx + d, cy + t, color);
            set_pixel(frame, cx + t, cy + d, color);
        }
    }
}

fn draw_minus(frame: &mut [u8], cx: i32, cy: i32, arm: i32, color: [u8; 4]) {
    for d in -arm..=arm {
        for t in -1i32..=1 {
            set_pixel(frame, cx + d, cy + t, color);
        }
    }
}

// ── Draw the editor frame ────────────────────────────────────────────────────

fn draw_editor(frame: &mut [u8], state: &EditorState) {
    fill_rect(frame, 0, 0, W, H, BG);

    let color_map = ColorMap::new(&state.keyframes, LinearInterpolator {});

    // ── Gradient bar ──────────────────────────────────────────────────────
    draw_colormap_gradient(frame, TL_X, GRAD_Y, TL_W, GRAD_H, &color_map);

    // ── Timeline ─────────────────────────────────────────────────────────
    fill_rect(frame, TL_X, TL_Y, TL_W, TL_H, BG_DARK);
    // Tick lines at 0, 0.25, 0.5, 0.75, 1.0
    for i in 0..=4u32 {
        let tx = TL_X as i32 + (i as f32 / 4.0 * TL_W as f32) as i32;
        for dy in 0..TL_H as i32 {
            set_pixel(frame, tx, TL_Y as i32 + dy, BG_MID);
        }
    }

    let stagger = compute_stagger(&state.keyframes);

    // Draw unselected markers first so selected renders on top.
    for (i, kf) in state.keyframes.iter().enumerate() {
        if state.selected == Some(i) {
            continue;
        }
        let cx = query_to_x(kf.query);
        let cy = stagger_cy(stagger[i]);
        let c = [kf.rgb_raw[0], kf.rgb_raw[1], kf.rgb_raw[2], 255];
        draw_circle(frame, cx, cy, MARKER_R, c);
        draw_ring(frame, cx, cy, MARKER_R, MARKER_R - 2, DIM);
    }
    if let Some(sel) = state.selected {
        let kf = &state.keyframes[sel];
        let cx = query_to_x(kf.query);
        let cy = stagger_cy(stagger[sel]);
        let c = [kf.rgb_raw[0], kf.rgb_raw[1], kf.rgb_raw[2], 255];
        draw_ring(frame, cx, cy, MARKER_SEL, MARKER_R, WHITE);
        draw_circle(frame, cx, cy, MARKER_R, c);
    }

    // ── Color picker ──────────────────────────────────────────────────────
    if let Some(sel) = state.selected {
        let [h, s, v] = state.hsv;
        let [r, g, b] = state.keyframes[sel].rgb_raw;

        // SV square
        draw_sv_square(frame, SV_X, SV_Y, SV_SIZE, h);
        // Crosshair at current S/V
        let sx = SV_X as i32 + (s * (SV_SIZE - 1) as f32).round() as i32;
        let sy = SV_Y as i32 + ((1.0 - v) * (SV_SIZE - 1) as f32).round() as i32;
        let ring_color = if v > 0.4 { [0u8, 0, 0, 255] } else { WHITE };
        draw_crosshair(frame, sx, sy, 7, ring_color);

        // H strip (vertical)
        draw_h_strip(frame, HS_X, SV_Y, HS_W, HS_H);
        // Arrow marker on left side of strip
        let hy = SV_Y as i32 + (h / 360.0 * (HS_H - 1) as f32).round() as i32;
        draw_arrow_right(frame, HS_X as i32 - 2, hy, 5, WHITE);

        // Color swatch
        fill_rect(frame, SWATCH_X, SV_Y, SWATCH_W, SWATCH_H, [r, g, b, 255]);
        draw_ring(frame,
            (SWATCH_X + SWATCH_W / 2) as i32,
            (SV_Y + SWATCH_H / 2) as i32,
            (SWATCH_W / 2) as i32,
            (SWATCH_W / 2 - 2) as i32,
            DIM);
    } else {
        // Dim placeholder when nothing is selected
        fill_rect(frame, SV_X, SV_Y, SV_SIZE, SV_SIZE, BG_MID);
        fill_rect(frame, HS_X, SV_Y, HS_W, HS_H, BG_MID);
        fill_rect(frame, SWATCH_X, SV_Y, SWATCH_W, SWATCH_H, BG_MID);
    }

    // ── Add / Remove buttons ──────────────────────────────────────────────
    fill_rect(frame, ADD_X, BTN_Y, BTN_SZ, BTN_SZ, ADD_COL);
    draw_plus(frame,
        (ADD_X + BTN_SZ / 2) as i32, (BTN_Y + BTN_SZ / 2) as i32, 8, WHITE);

    fill_rect(frame, REM_X, BTN_Y, BTN_SZ, BTN_SZ, REM_COL);
    draw_minus(frame,
        (REM_X + BTN_SZ / 2) as i32, (BTN_Y + BTN_SZ / 2) as i32, 8, WHITE);
}

// ── Fractal preview ──────────────────────────────────────────────────────────

fn draw_preview(frame: &mut [u8], buffer: &[Vec<image::Rgb<u8>>], pw: u32, ph: u32) {
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

fn scale_preview(spec: &ImageSpecification, max_w: u32, max_h: u32) -> ImageSpecification {
    let scale = (max_w as f64 / spec.resolution[0] as f64)
        .min(max_h as f64 / spec.resolution[1] as f64)
        .min(1.0);
    let pw = ((spec.resolution[0] as f64 * scale).round() as u32).max(1);
    let ph = ((spec.resolution[1] as f64 * scale).round() as u32).max(1);
    ImageSpecification { resolution: [pw, ph], center: spec.center, width: spec.width }
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Open the two-window color-map editor.
///
/// * `renderer`  – fractal renderer (must implement `Renderable` + `ColorMapEditable`)
/// * `save_fn`   – called with the final keyframes on 'S' or window close
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
         \x20 • ←/→ arrow keys – cycle through keyframes\n\
         \x20 • Shift+←/→      – nudge query value 10 % toward neighbor\n\
         \x20 • Click/drag the SV square to change saturation & value\n\
         \x20 • Click/drag the hue strip to change hue\n\
         \x20 • N or Enter     – add keyframe at midpoint\n\
         \x20 • Delete/Backspace – remove selected keyframe\n\
         \x20 • S              – save params back to file\n\
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
    let editor_wid  = editor_window.id();

    let mut fractal_pixels = {
        let sz = fractal_window.inner_size();
        Pixels::new(pw, ph, SurfaceTexture::new(sz.width, sz.height, &fractal_window))?
    };
    let mut editor_pixels = {
        let sz = editor_window.inner_size();
        Pixels::new(W, H, SurfaceTexture::new(sz.width, sz.height, &editor_window))?
    };

    let mut renderer = renderer;
    renderer.set_image_specification(preview_spec);

    let renderer      = Arc::new(Mutex::new(renderer));
    let display_buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>> =
        Arc::new(Mutex::new(create_buffer(image::Rgb([0u8, 0, 0]), &[pw, ph])));
    let render_busy  = Arc::new(AtomicBool::new(false));
    let render_ready = Arc::new(AtomicBool::new(false));

    let mut state = EditorState::new(initial_keyframes);

    spawn_render(renderer.clone(), display_buffer.clone(), render_busy.clone(), render_ready.clone());

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
                // Keyboard events work from either window so focus doesn't matter.
                WindowEvent::KeyboardInput { .. } | WindowEvent::ModifiersChanged(_) => {
                    state.handle_window_event(event);
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

        // ── Redraws ───────────────────────────────────────────────────────
        if let Event::RedrawRequested(wid) = event {
            if wid == fractal_wid {
                draw_preview(fractal_pixels.frame_mut(), &display_buffer.lock().unwrap(), pw, ph);
                fractal_pixels.render().ok();
            } else if wid == editor_wid {
                draw_editor(editor_pixels.frame_mut(), &state);
                editor_pixels.render().ok();
            }
        }

        // ── Main tick ─────────────────────────────────────────────────────
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

            let need_editor = state.view_dirty || state.keyframes_dirty;
            state.view_dirty = false;

            if state.keyframes_dirty {
                state.keyframes_dirty = false;
                renderer.lock().unwrap().set_keyframes(state.keyframes.clone());
                spawn_render(renderer.clone(), display_buffer.clone(),
                             render_busy.clone(), render_ready.clone());
            }

            if render_ready.swap(false, Ordering::AcqRel) {
                fractal_window.request_redraw();
            }
            if need_editor {
                editor_window.request_redraw();
            }

            let active = render_busy.load(Ordering::Acquire) || state.mouse_down;
            *control_flow = if active {
                ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16))
            } else {
                ControlFlow::Wait
            };
        }
    });
}
