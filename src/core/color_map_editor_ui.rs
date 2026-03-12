use pixels::{Error, Pixels, SurfaceTexture};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
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

// ── Editor window dimensions ─────────────────────────────────────────────────
const W: u32 = 900;
const H: u32 = 520;

// ── Static layout ─────────────────────────────────────────────────────────────
const GRAD_X: u32 = 16;
const GRAD_W: u32 = 868;   // full usable width (W - 2×16)
const GRAD_H: u32 = 44;
const GRAD_Y: u32 = 12;

const TL_Y: u32 = 68;      // timeline section top
const TRACK_H: u32 = 26;   // height per keyframe track
const TRACK_MARKER_W: u32 = 796; // width of the marker area within each track
const QUERY_BOX_X: u32 = GRAD_X + TRACK_MARKER_W + 8;
const QUERY_BOX_W: u32 = W - QUERY_BOX_X - 16; // ≈ 64

const MARKER_R: i32 = 9;
const MARKER_SEL: i32 = 13;

// Color-picker bars (two columns: HSV left, RGB right)
const COL_W:    u32 = 426;
const COL_L_X:  u32 = GRAD_X;
const COL_R_X:  u32 = COL_L_X + COL_W + 16;
const BAR_LABEL_W: u32 = 20;
const BAR_SL_OFF:  u32 = BAR_LABEL_W + 8;  // slider left offset within col
const BAR_SL_W:    u32 = COL_W - BAR_LABEL_W - 8 - 4 - 50; // ≈ 344
const BAR_VAL_OFF: u32 = BAR_SL_OFF + BAR_SL_W + 4;
const BAR_VAL_W:   u32 = 50;
const BAR_H:    u32 = 22;
const BAR_ROW_H: u32 = BAR_H + 8;

// Buttons
const BTN_SZ:  u32 = 32;
const ADD_X:   u32 = GRAD_X;
const REM_X:   u32 = ADD_X + BTN_SZ + 10;

// Rendering speed: level for fast color-preview vs full quality
const FAST_LEVEL: f64 = 0.7;
const FULL_LEVEL: f64 = 0.0;

// ── Palette ──────────────────────────────────────────────────────────────────
const BG:      [u8; 4] = [22,  22,  32,  255];
const BG_DARK: [u8; 4] = [14,  14,  22,  255];
const BG_MID:  [u8; 4] = [38,  38,  52,  255];
const WHITE:   [u8; 4] = [255, 255, 255, 255];
const DIM:     [u8; 4] = [70,  70,  90,  255];
const ACTIVE_BG:[u8;4] = [50,  50,  80,  255];
const SEL_BG:  [u8; 4] = [30,  30,  50,  255];
const ADD_COL: [u8; 4] = [50,  160,  70, 255];
const REM_COL: [u8; 4] = [180,  50,  50, 255];
const ORANGE:  [u8; 4] = [220, 130,  20, 255];

// ── Bitmap font (5×7, scale-able) ────────────────────────────────────────────
// Each glyph: 7 bytes, one per row top→bottom.
// Each byte: bits [4..0] = pixels left→right; bit 4 = leftmost column.
fn glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [0x0E,0x11,0x11,0x11,0x11,0x11,0x0E],
        '1' => [0x04,0x0C,0x04,0x04,0x04,0x04,0x0E],
        '2' => [0x0E,0x11,0x01,0x02,0x04,0x08,0x1F],
        '3' => [0x1F,0x02,0x04,0x02,0x01,0x11,0x0E],
        '4' => [0x02,0x06,0x0A,0x12,0x1F,0x02,0x02],
        '5' => [0x1F,0x10,0x1E,0x01,0x01,0x11,0x0E],
        '6' => [0x06,0x08,0x10,0x1E,0x11,0x11,0x0E],
        '7' => [0x1F,0x01,0x02,0x04,0x08,0x08,0x08],
        '8' => [0x0E,0x11,0x11,0x0E,0x11,0x11,0x0E],
        '9' => [0x0E,0x11,0x11,0x0F,0x01,0x02,0x0C],
        '.' => [0x00,0x00,0x00,0x00,0x00,0x06,0x06],
        '-' => [0x00,0x00,0x00,0x1F,0x00,0x00,0x00],
        '/' => [0x01,0x02,0x04,0x08,0x10,0x00,0x00],
        ' ' => [0x00;7],
        'R' => [0x1E,0x11,0x11,0x1E,0x14,0x12,0x11],
        'G' => [0x0E,0x10,0x10,0x17,0x11,0x11,0x0F],
        'B' => [0x1E,0x11,0x11,0x1E,0x11,0x11,0x1E],
        'H' => [0x11,0x11,0x11,0x1F,0x11,0x11,0x11],
        'S' => [0x0E,0x10,0x10,0x0E,0x01,0x01,0x0E],
        'V' => [0x11,0x11,0x11,0x11,0x0A,0x0A,0x04],
        _ =>   [0x00;7],
    }
}

// ── HSV conversion ────────────────────────────────────────────────────────────

fn rgb_to_hsv(rgb: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 1e-6 { 0.0 }
            else if max == r { 60.0 * ((g - b) / d).rem_euclid(6.0) }
            else if max == g { 60.0 * ((b - r) / d + 2.0) }
            else             { 60.0 * ((r - g) / d + 4.0) };
    [h, if max < 1e-6 { 0.0 } else { d / max }, max]
}

fn hsv_to_rgb(hsv: [f32; 3]) -> [u8; 3] {
    let [h, s, v] = [hsv[0], hsv[1].clamp(0.0,1.0), hsv[2].clamp(0.0,1.0)];
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c,x,0.0), 1 => (x,c,0.0), 2 => (0.0,c,x),
        3 => (0.0,x,c), 4 => (x,0.0,c), _ => (c,0.0,x),
    };
    [((r+m)*255.0).round() as u8, ((g+m)*255.0).round() as u8, ((b+m)*255.0).round() as u8]
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum SlotId { H, S, V, R, G, B }

#[derive(Debug, Clone, Copy, PartialEq)]
enum FieldId { Query(usize), Slot(SlotId) }

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag { None, Marker(usize), Slider(SlotId) }

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode { Color, Rendering }

struct TextInput { field: FieldId, text: String }

// ── Layout helpers ────────────────────────────────────────────────────────────

fn track_y(i: usize) -> u32 { TL_Y + i as u32 * TRACK_H }
fn track_cy(i: usize) -> i32 { (track_y(i) + TRACK_H / 2) as i32 }

fn color_y(n_kf: usize) -> u32 { TL_Y + n_kf as u32 * TRACK_H + 16 }
fn bar_y(n_kf: usize, idx: u32) -> u32 { color_y(n_kf) + idx * BAR_ROW_H }
fn btn_y(n_kf: usize) -> u32 { color_y(n_kf) + 3 * BAR_ROW_H + 12 }

fn query_to_x(q: f32) -> i32 {
    GRAD_X as i32 + (q * TRACK_MARKER_W as f32).round() as i32
}
fn x_to_query(x: i32) -> f32 {
    ((x - GRAD_X as i32) as f32 / TRACK_MARKER_W as f32).clamp(0.0, 1.0)
}

fn hit_rect(px: f32, py: f32, x: u32, y: u32, w: u32, h: u32) -> bool {
    px >= x as f32 && px < (x+w) as f32 && py >= y as f32 && py < (y+h) as f32
}

// ── Editor state ──────────────────────────────────────────────────────────────

struct EditorState {
    keyframes:      Vec<ColorMapKeyFrame>,
    selected:       Option<usize>,
    hsv:            [f32; 3],
    drag:           Drag,
    keyframes_dirty: bool,
    view_dirty:     bool,
    save_requested: bool,
    quit_requested: bool,
    cursor:         (f64, f64),
    mouse_down:     bool,
    modifiers:      ModifiersState,
    mode:           Mode,
    full_render_requested: bool,
    active_input:   Option<TextInput>,
    render_start:   Option<Instant>,
}

impl EditorState {
    fn new(keyframes: Vec<ColorMapKeyFrame>) -> Self {
        Self {
            keyframes,
            selected:       None,
            hsv:            [0.0, 0.0, 1.0],
            drag:           Drag::None,
            keyframes_dirty: true,
            view_dirty:     false,
            save_requested: false,
            quit_requested: false,
            cursor:         (0.0, 0.0),
            mouse_down:     false,
            modifiers:      ModifiersState::default(),
            mode:                  Mode::Color,
            full_render_requested: false,
            active_input:          None,
            render_start:          None,
        }
    }

    fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(m) => self.modifiers = *m,

            WindowEvent::ReceivedCharacter(c) => {
                if let Some(ref mut inp) = self.active_input {
                    if c.is_ascii_digit() || *c == '.' {
                        inp.text.push(*c);
                        self.view_dirty = true;
                    }
                }
            }

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
                if input.state != ElementState::Pressed { return; }
                let key = match input.virtual_keycode { Some(k) => k, None => return };

                // Text-input mode consumes most keys
                if self.active_input.is_some() {
                    match key {
                        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                            self.commit_input();
                        }
                        VirtualKeyCode::Escape => {
                            self.active_input = None;
                            self.view_dirty = true;
                        }
                        VirtualKeyCode::Back => {
                            if let Some(ref mut inp) = self.active_input {
                                inp.text.pop();
                                self.view_dirty = true;
                            }
                        }
                        _ => {}
                    }
                    return;
                }

                // Normal key handling
                match key {
                    VirtualKeyCode::Escape   => self.quit_requested = true,
                    VirtualKeyCode::S | VirtualKeyCode::Space => self.save_requested = true,
                    VirtualKeyCode::N => self.add_keyframe(),
                    // Enter → full quality render
                    VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                        self.full_render_requested = true;
                    }
                    VirtualKeyCode::Delete | VirtualKeyCode::Back => self.remove_selected(),
                    VirtualKeyCode::Left if self.modifiers.shift() => self.nudge_query(-1),
                    VirtualKeyCode::Right if self.modifiers.shift() => self.nudge_query(1),
                    VirtualKeyCode::Left  => self.cycle_selected(-1),
                    VirtualKeyCode::Right => self.cycle_selected(1),
                    _ => {}
                }
            }

            WindowEvent::Focused(false) => {
                self.mouse_down = false;
                self.drag = Drag::None;
            }
            _ => {}
        }
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        let n = self.keyframes.len();
        let btn = btn_y(n);

        // Commit any active text input when clicking elsewhere
        if let Some(ref inp) = self.active_input {
            let field = inp.field;
            if !self.is_click_on_field(x, y, field) {
                self.commit_input();
            }
        }

        // Buttons
        if hit_rect(x, y, ADD_X, btn, BTN_SZ, BTN_SZ) { self.add_keyframe(); return; }
        if hit_rect(x, y, REM_X, btn, BTN_SZ, BTN_SZ) { self.remove_selected(); return; }

        // Query-value text boxes
        for i in 0..n {
            let ty = track_y(i);
            if hit_rect(x, y, QUERY_BOX_X, ty, QUERY_BOX_W, TRACK_H) {
                self.activate_input(FieldId::Query(i));
                return;
            }
        }

        // Keyframe markers (prefer selected on tie)
        let mut best: Option<(usize, i32)> = None;
        for (i, kf) in self.keyframes.iter().enumerate() {
            let mx = query_to_x(kf.query);
            let my = track_cy(i);
            let dx = x as i32 - mx;
            let dy = y as i32 - my;
            let d2 = dx*dx + dy*dy;
            let hit = MARKER_SEL + 4;
            if d2 <= hit*hit {
                let wins = best.map_or(true, |(pi, pd2)| {
                    let sel = |idx: usize| if self.selected == Some(idx) { 0 } else { 1 };
                    (sel(i), d2) < (sel(pi), pd2)
                });
                if wins { best = Some((i, d2)); }
            }
        }
        if let Some((i, _)) = best {
            self.set_selected(Some(i));
            self.drag = Drag::Marker(i);
            return;
        }

        // Color picker sliders + value boxes (only when keyframe selected)
        eprintln!("DEBUG click: x={x:.1} y={y:.1} selected={:?}", self.selected);
        if self.selected.is_some() {
            let sliders = [
                (SlotId::H, COL_L_X, 0u32),
                (SlotId::S, COL_L_X, 1),
                (SlotId::V, COL_L_X, 2),
                (SlotId::R, COL_R_X, 0),
                (SlotId::G, COL_R_X, 1),
                (SlotId::B, COL_R_X, 2),
            ];
            for (slot, col_x, row) in sliders {
                let by = bar_y(n, row);
                let sl_x = col_x + BAR_SL_OFF;
                let vb_x = col_x + BAR_VAL_OFF;
                eprintln!("  check {slot:?}: slider=[{sl_x}..{}] valbox=[{vb_x}..{}] y=[{by}..{}]",
                    sl_x+BAR_SL_W, vb_x+BAR_VAL_W, by+BAR_H);
                if hit_rect(x, y, sl_x, by, BAR_SL_W, BAR_H) {
                    eprintln!("  -> slider hit {slot:?}");
                    self.drag = Drag::Slider(slot);
                    self.apply_slider_drag(x, sl_x, slot);
                    return;
                }
                if hit_rect(x, y, vb_x, by, BAR_VAL_W, BAR_H) {
                    eprintln!("  -> valbox hit {slot:?}");
                    self.activate_input(FieldId::Slot(slot));
                    return;
                }
            }
            eprintln!("  -> no color picker match");
        }

        // Click on timeline background → deselect
        let nt_h = n as u32 * TRACK_H;
        if hit_rect(x, y, GRAD_X, TL_Y, GRAD_W, nt_h) {
            self.set_selected(None);
        }
    }

    fn is_click_on_field(&self, x: f32, y: f32, field: FieldId) -> bool {
        let n = self.keyframes.len();
        match field {
            FieldId::Query(i) => hit_rect(x, y, QUERY_BOX_X, track_y(i), QUERY_BOX_W, TRACK_H),
            FieldId::Slot(slot) => {
                let (col_x, row) = slot_layout(slot);
                let by = bar_y(n, row);
                hit_rect(x, y, col_x + BAR_VAL_OFF, by, BAR_VAL_W, BAR_H)
            }
        }
    }

    fn activate_input(&mut self, field: FieldId) {
        let text = match field {
            FieldId::Query(i) => format!("{:.4}", self.keyframes[i].query),
            FieldId::Slot(SlotId::H) => format!("{:.1}", self.hsv[0]),
            FieldId::Slot(SlotId::S) => format!("{:.1}", self.hsv[1] * 100.0),
            FieldId::Slot(SlotId::V) => format!("{:.1}", self.hsv[2] * 100.0),
            FieldId::Slot(s) => {
                let rgb = self.selected_rgb().unwrap_or([0,0,0]);
                format!("{}", match s { SlotId::R => rgb[0], SlotId::G => rgb[1], _ => rgb[2] })
            }
        };
        self.active_input = Some(TextInput { field, text });
        self.view_dirty = true;
    }

    fn commit_input(&mut self) {
        let Some(inp) = self.active_input.take() else { return };
        match inp.field {
            FieldId::Query(i) => {
                if let Ok(v) = inp.text.parse::<f32>() {
                    let n = self.keyframes.len();
                    if i > 0 && i < n - 1 {
                        let lo = self.keyframes[i-1].query + 1e-4;
                        let hi = self.keyframes[i+1].query - 1e-4;
                        self.keyframes[i].query = v.clamp(lo, hi);
                        self.keyframes_dirty = true;
                    }
                }
            }
            FieldId::Slot(slot) => {
                let Some(sel) = self.selected else { return };
                match slot {
                    SlotId::H => if let Ok(v) = inp.text.parse::<f32>() {
                        self.hsv[0] = v.clamp(0.0, 360.0);
                        self.flush_hsv();
                    },
                    SlotId::S => if let Ok(v) = inp.text.parse::<f32>() {
                        self.hsv[1] = (v / 100.0).clamp(0.0, 1.0);
                        self.flush_hsv();
                    },
                    SlotId::V => if let Ok(v) = inp.text.parse::<f32>() {
                        self.hsv[2] = (v / 100.0).clamp(0.0, 1.0);
                        self.flush_hsv();
                    },
                    SlotId::R => if let Ok(v) = inp.text.parse::<u8>() {
                        self.keyframes[sel].rgb_raw[0] = v;
                        self.sync_hsv(); self.keyframes_dirty = true;
                    },
                    SlotId::G => if let Ok(v) = inp.text.parse::<u8>() {
                        self.keyframes[sel].rgb_raw[1] = v;
                        self.sync_hsv(); self.keyframes_dirty = true;
                    },
                    SlotId::B => if let Ok(v) = inp.text.parse::<u8>() {
                        self.keyframes[sel].rgb_raw[2] = v;
                        self.sync_hsv(); self.keyframes_dirty = true;
                    },
                }
            }
        }
        self.view_dirty = true;
    }

    fn apply_drag(&mut self, x: f32, _y: f32) {
        match self.drag {
            Drag::None => {}
            Drag::Marker(i) => self.drag_marker(i, x),
            Drag::Slider(slot) => {
                let (col_x, _) = slot_layout(slot);
                self.apply_slider_drag(x, col_x + BAR_SL_OFF, slot);
            }
        }
    }

    fn drag_marker(&mut self, i: usize, x: f32) {
        let n = self.keyframes.len();
        if i == 0 || i == n - 1 { return; }
        let lo = self.keyframes[i-1].query + 0.001;
        let hi = self.keyframes[i+1].query - 0.001;
        let q = x_to_query(x as i32).clamp(lo, hi);
        if (self.keyframes[i].query - q).abs() > 1e-6 {
            self.keyframes[i].query = q;
            self.keyframes_dirty = true;
        }
    }

    fn apply_slider_drag(&mut self, x: f32, sl_x: u32, slot: SlotId) {
        let t = ((x - sl_x as f32) / BAR_SL_W as f32).clamp(0.0, 1.0);
        match slot {
            SlotId::H => { self.hsv[0] = t * 360.0; self.flush_hsv(); }
            SlotId::S => { self.hsv[1] = t;          self.flush_hsv(); }
            SlotId::V => { self.hsv[2] = t;          self.flush_hsv(); }
            SlotId::R | SlotId::G | SlotId::B => {
                let Some(sel) = self.selected else { return };
                let v = (t * 255.0).round() as u8;
                let ch = match slot { SlotId::R => 0, SlotId::G => 1, _ => 2 };
                if self.keyframes[sel].rgb_raw[ch] != v {
                    self.keyframes[sel].rgb_raw[ch] = v;
                    self.sync_hsv();
                    self.keyframes_dirty = true;
                }
            }
        }
    }

    fn flush_hsv(&mut self) {
        let Some(sel) = self.selected else { return };
        let new_rgb = hsv_to_rgb(self.hsv);
        if self.keyframes[sel].rgb_raw != new_rgb {
            self.keyframes[sel].rgb_raw = new_rgb;
            self.keyframes_dirty = true;
        }
    }

    fn sync_hsv(&mut self) {
        if let Some(sel) = self.selected {
            self.hsv = rgb_to_hsv(self.keyframes[sel].rgb_raw);
        }
    }

    fn set_selected(&mut self, idx: Option<usize>) {
        if self.selected != idx {
            self.selected = idx;
            self.sync_hsv();
            self.view_dirty = true;
        }
    }

    fn cycle_selected(&mut self, dir: i32) {
        let n = self.keyframes.len() as i32;
        let cur = self.selected.map(|i| i as i32).unwrap_or(-1);
        self.set_selected(Some(((cur + dir).rem_euclid(n)) as usize));
    }

    fn nudge_query(&mut self, dir: i32) {
        let Some(sel) = self.selected else { return };
        let n = self.keyframes.len();
        if sel == 0 || sel == n - 1 { return; }
        let cur = self.keyframes[sel].query;
        let lo = self.keyframes[sel-1].query;
        let hi = self.keyframes[sel+1].query;
        let new_q = if dir < 0 { cur - 0.1*(cur - lo) } else { cur + 0.1*(hi - cur) };
        let new_q = new_q.clamp(lo + 1e-5, hi - 1e-5);
        if (new_q - cur).abs() > 1e-7 {
            self.keyframes[sel].query = new_q;
            self.keyframes_dirty = true;
        }
    }

    fn add_keyframe(&mut self) {
        let (ins, lo, hi) = if let Some(sel) = self.selected {
            let lo = self.keyframes[sel].query;
            let hi = self.keyframes.get(sel+1).map_or(1.0, |k| k.query);
            (sel+1, lo, hi)
        } else {
            let (bi, _) = (0..self.keyframes.len()-1)
                .map(|i| (i, self.keyframes[i+1].query - self.keyframes[i].query))
                .max_by(|a,b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap_or((0, 0.0));
            (bi+1, self.keyframes[bi].query, self.keyframes[bi+1].query)
        };
        if hi - lo < 0.002 { return; }
        let q = 0.5*(lo + hi);
        let c0 = self.keyframes[ins-1].rgb_raw;
        let c1 = self.keyframes[ins].rgb_raw;
        let lerp = |a: u8, b: u8| (0.5*(a as f32 + b as f32)).round() as u8;
        let rgb = [lerp(c0[0],c1[0]), lerp(c0[1],c1[1]), lerp(c0[2],c1[2])];
        self.keyframes.insert(ins, ColorMapKeyFrame { query: q, rgb_raw: rgb });
        self.set_selected(Some(ins));
        self.keyframes_dirty = true;
    }

    fn remove_selected(&mut self) {
        let Some(sel) = self.selected else { return };
        let n = self.keyframes.len();
        if sel == 0 || sel == n-1 || n <= 2 { return; }
        self.keyframes.remove(sel);
        let new = sel.min(self.keyframes.len() - 2);
        self.set_selected(Some(new));
        self.keyframes_dirty = true;
    }

    fn selected_rgb(&self) -> Option<[u8; 3]> {
        self.selected.map(|s| self.keyframes[s].rgb_raw)
    }
}

/// Return (col_x, bar_row) for a given slot.
fn slot_layout(slot: SlotId) -> (u32, u32) {
    match slot {
        SlotId::H => (COL_L_X, 0),
        SlotId::S => (COL_L_X, 1),
        SlotId::V => (COL_L_X, 2),
        SlotId::R => (COL_R_X, 0),
        SlotId::G => (COL_R_X, 1),
        SlotId::B => (COL_R_X, 2),
    }
}

// ── Canvas ────────────────────────────────────────────────────────────────────

struct Canvas<'a> { frame: &'a mut [u8] }

impl<'a> Canvas<'a> {
    fn set_pixel(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 { return; }
        let i = ((y as u32 * W + x as u32) * 4) as usize;
        self.frame[i..i+4].copy_from_slice(&c);
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, c: [u8; 4]) {
        for py in y..(y+h).min(H) {
            for px in x..(x+w).min(W) {
                let i = ((py * W + px) * 4) as usize;
                self.frame[i..i+4].copy_from_slice(&c);
            }
        }
    }

    fn colormap_gradient(&mut self, x: u32, y: u32, w: u32, h: u32,
                          cm: &ColorMap<LinearInterpolator>) {
        for px in x..(x+w).min(W) {
            let t = if w > 1 { (px-x) as f32 / (w-1) as f32 } else { 0.0 };
            let rgb = cm.compute_pixel(t);
            let c = [rgb[0], rgb[1], rgb[2], 255];
            for py in y..(y+h).min(H) {
                let i = ((py * W + px) * 4) as usize;
                self.frame[i..i+4].copy_from_slice(&c);
            }
        }
    }

    fn circle(&mut self, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
        let r2 = r*r;
        for dy in -r..=r { for dx in -r..=r {
            if dx*dx+dy*dy <= r2 { self.set_pixel(cx+dx, cy+dy, c); }
        }}
    }

    fn ring(&mut self, cx: i32, cy: i32, outer: i32, inner: i32, c: [u8; 4]) {
        let (o2,i2) = (outer*outer, inner*inner);
        for dy in -outer..=outer { for dx in -outer..=outer {
            let d2 = dx*dx+dy*dy;
            if d2 <= o2 && d2 > i2 { self.set_pixel(cx+dx, cy+dy, c); }
        }}
    }

    fn thumb(&mut self, x: i32, y: u32, h: u32, c: [u8; 4]) {
        for dy in 0..h as i32 {
            for dx in -2i32..=2 { self.set_pixel(x+dx, y as i32+dy, c); }
        }
    }

    fn glyph(&mut self, ch: char, x: u32, y: u32, scale: u32, c: [u8; 4]) {
        let bits = glyph(ch);
        for (row, &byte) in bits.iter().enumerate() {
            for col in 0..5u32 {
                if byte & (1 << (4-col)) != 0 {
                    self.fill_rect(x+col*scale, y+row as u32*scale, scale, scale, c);
                }
            }
        }
    }

    /// Draw string; returns pixel width of drawn text (char_w = (5+1)*scale).
    fn text(&mut self, s: &str, x: i32, y: u32, scale: u32, c: [u8; 4]) -> u32 {
        let cw = (5 + 1) * scale;
        for (i, ch) in s.chars().enumerate() {
            let cx = x + i as i32 * cw as i32;
            if cx >= 0 && cx < W as i32 { self.glyph(ch, cx as u32, y, scale, c); }
        }
        s.len() as u32 * cw
    }

    fn text_box(&mut self, text: &str, x: u32, y: u32, w: u32, h: u32,
                 active: bool, cursor: bool) {
        let bg = if active { ACTIVE_BG } else { BG_DARK };
        self.fill_rect(x, y, w, h, bg);
        let border = if active { WHITE } else { DIM };
        // top+bottom
        self.fill_rect(x, y, w, 1, border);
        self.fill_rect(x, y+h-1, w, 1, border);
        // sides
        for py in y..y+h { self.set_pixel(x as i32, py as i32, border); self.set_pixel((x+w-1) as i32, py as i32, border); }
        // text content (scale 2, padded 3px from left)
        let text_y = y + (h - 14)/2; // center 14px tall text
        let tw = self.text(text, (x+3) as i32, text_y, 2, WHITE);
        // blinking cursor
        if cursor {
            let cx = x + 3 + tw;
            self.fill_rect(cx, text_y, 2, 14, WHITE);
        }
    }

    fn plus(&mut self, cx: i32, cy: i32, arm: i32, c: [u8; 4]) {
        for d in -arm..=arm {
            for t in -1i32..=1 {
                self.set_pixel(cx+d, cy+t, c);
                self.set_pixel(cx+t, cy+d, c);
            }
        }
    }

    fn minus(&mut self, cx: i32, cy: i32, arm: i32, c: [u8; 4]) {
        for d in -arm..=arm { for t in -1i32..=1 { self.set_pixel(cx+d, cy+t, c); } }
    }
}

// ── Draw the editor ───────────────────────────────────────────────────────────

fn draw_editor(frame: &mut [u8], state: &EditorState) {
    let mut cv = Canvas { frame };
    cv.fill_rect(0, 0, W, H, BG);

    let n = state.keyframes.len();
    let color_map = ColorMap::new(&state.keyframes, LinearInterpolator {});

    // ── Gradient bar ──────────────────────────────────────────────────────
    cv.colormap_gradient(GRAD_X, GRAD_Y, GRAD_W, GRAD_H, &color_map);

    // ── Per-keyframe timeline tracks ─────────────────────────────────────
    let tick_xs: Vec<i32> = (0..=4).map(|i| GRAD_X as i32 + (i as f32/4.0*TRACK_MARKER_W as f32) as i32).collect();

    for (i, kf) in state.keyframes.iter().enumerate() {
        let ty = track_y(i);
        let cy = track_cy(i);
        let is_sel = state.selected == Some(i);

        // Track background
        cv.fill_rect(GRAD_X, ty, GRAD_W, TRACK_H, if is_sel { SEL_BG } else { BG_DARK });

        // Subtle tick lines
        for &tx in &tick_xs {
            for dy in 0..TRACK_H as i32 { cv.set_pixel(tx, ty as i32 + dy, BG_MID); }
        }

        // Marker
        let mx = query_to_x(kf.query);
        let mc = [kf.rgb_raw[0], kf.rgb_raw[1], kf.rgb_raw[2], 255];
        if is_sel {
            cv.ring(mx, cy, MARKER_SEL, MARKER_R, WHITE);
        }
        cv.circle(mx, cy, MARKER_R, mc);
        if !is_sel { cv.ring(mx, cy, MARKER_R, MARKER_R-2, DIM); }

        // Query value box
        let qstr = format!("{:.4}", kf.query);
        let q_active = state.active_input.as_ref()
            .map_or(false, |inp| inp.field == FieldId::Query(i));
        let display = if q_active {
            state.active_input.as_ref().unwrap().text.clone()
        } else { qstr };
        cv.text_box(&display, QUERY_BOX_X, ty + (TRACK_H-22)/2, QUERY_BOX_W, 22,
                    q_active, q_active);
    }

    // ── Color picker (HSV left, RGB right) ───────────────────────────────
    if let Some(sel) = state.selected {
        let [h, s, v] = state.hsv;
        let [r, g, b] = state.keyframes[sel].rgb_raw;

        let bars: &[(SlotId, char, Box<dyn Fn(f32)->[u8;3]>, Box<dyn Fn()->[u8;3]>, f32)] = &[
            (SlotId::H, 'H', Box::new(|t| hsv_to_rgb([t*360.0, 1.0, 1.0])), Box::new(|| hsv_to_rgb([h,s,v])), h/360.0),
            (SlotId::S, 'S', Box::new(move|t| hsv_to_rgb([h, t, v])),        Box::new(|| hsv_to_rgb([h,s,v])), s),
            (SlotId::V, 'V', Box::new(move|t| hsv_to_rgb([h, s, t])),        Box::new(|| hsv_to_rgb([h,s,v])), v),
            (SlotId::R, 'R', Box::new(move|t| [(t*255.0) as u8, g, b]),       Box::new(|| [r,g,b]), r as f32/255.0),
            (SlotId::G, 'G', Box::new(move|t| [r, (t*255.0) as u8, b]),       Box::new(|| [r,g,b]), g as f32/255.0),
            (SlotId::B, 'B', Box::new(move|t| [r, g, (t*255.0) as u8]),       Box::new(|| [r,g,b]), b as f32/255.0),
        ];

        for (slot, label, grad_fn, _, thumb_t) in bars.iter() {
            let (col_x, row) = slot_layout(*slot);
            let by = bar_y(n, row);
            let sl_x = col_x + BAR_SL_OFF;
            let vb_x = col_x + BAR_VAL_OFF;

            // Label
            cv.glyph(*label, col_x + 5, by + (BAR_H - 14) / 2, 2, WHITE);

            // Gradient slider
            for px in sl_x..(sl_x + BAR_SL_W).min(W) {
                let t = (px - sl_x) as f32 / (BAR_SL_W - 1) as f32;
                let rgb = grad_fn(t);
                let c = [rgb[0], rgb[1], rgb[2], 255];
                for py in by..by+BAR_H { let i = ((py*W+px)*4) as usize; cv.frame[i..i+4].copy_from_slice(&c); }
            }

            // Thumb
            let tx = sl_x as i32 + (thumb_t * (BAR_SL_W-1) as f32).round() as i32;
            cv.thumb(tx, by, BAR_H, WHITE);

            // Value text box
            let is_active = state.active_input.as_ref()
                .map_or(false, |inp| inp.field == FieldId::Slot(*slot));
            let val_str = if is_active {
                state.active_input.as_ref().unwrap().text.clone()
            } else {
                match slot {
                    SlotId::H => format!("{:.1}", h),
                    SlotId::S => format!("{:.1}", s * 100.0),
                    SlotId::V => format!("{:.1}", v * 100.0),
                    SlotId::R => format!("{r}"),
                    SlotId::G => format!("{g}"),
                    SlotId::B => format!("{b}"),
                }
            };
            cv.text_box(&val_str, vb_x, by, BAR_VAL_W, BAR_H, is_active, is_active);
        }
    } else {
        // Dim placeholder
        for row in 0..3u32 {
            let by_l = bar_y(n, row);
            let by_r = bar_y(n, row);
            cv.fill_rect(COL_L_X + BAR_SL_OFF, by_l, BAR_SL_W, BAR_H, BG_MID);
            cv.fill_rect(COL_R_X + BAR_SL_OFF, by_r, BAR_SL_W, BAR_H, BG_MID);
        }
    }

    // ── Buttons ───────────────────────────────────────────────────────────
    let btn = btn_y(n);
    cv.fill_rect(ADD_X, btn, BTN_SZ, BTN_SZ, ADD_COL);
    cv.plus((ADD_X + BTN_SZ/2) as i32, (btn + BTN_SZ/2) as i32, 8, WHITE);
    cv.fill_rect(REM_X, btn, BTN_SZ, BTN_SZ, REM_COL);
    cv.minus((REM_X + BTN_SZ/2) as i32, (btn + BTN_SZ/2) as i32, 8, WHITE);

    // ── Rendering-mode indicator ──────────────────────────────────────────
    if state.mode == Mode::Rendering {
        let elapsed = state.render_start.map_or(0, |t| t.elapsed().as_millis() as u32);
        let phase = ((elapsed % 1000) as f32) / 500.0 - 1.0; // -1..1
        let pulse = 1.0 - phase.abs(); // 0..1..0
        let bright = (50.0 + pulse * 180.0) as u8;
        let strip_y = H - 8;
        cv.fill_rect(0, strip_y, W, 8, [200, bright, 0, 255]);
        let _ = ORANGE; // keep const used
    }
}

// ── Fractal preview ───────────────────────────────────────────────────────────

fn draw_preview(frame: &mut [u8], buf: &[Vec<image::Rgb<u8>>], pw: u32, _ph: u32) {
    for (flat, pixel) in frame.chunks_exact_mut(4).enumerate() {
        let x = flat as u32 % pw;
        let y = flat as u32 / pw;
        if (x as usize) < buf.len() && (y as usize) < buf[x as usize].len() {
            let rgb = buf[x as usize][y as usize];
            pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
}

/// Attempt to start a background render; returns `false` if busy (no-op).
fn spawn_render<F: Renderable + Send + 'static>(
    renderer: Arc<Mutex<F>>,
    buffer:   Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>>,
    busy:     Arc<AtomicBool>,
    ready:    Arc<AtomicBool>,
) -> bool {
    if busy.swap(true, Ordering::Acquire) { return false; }
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

pub fn edit<F, Save>(renderer: F, save_fn: Save) -> Result<(), Error>
where
    F: Renderable + ColorMapEditable + Send + Sync + 'static,
    F::ReferenceCache: Send + 'static,
    Save: Fn(&[ColorMapKeyFrame]) + 'static,
{
    let initial_keyframes = renderer.get_keyframes();
    // Use the full resolution but cap preview window size;
    // downsample stride handles fast preview rendering.
    let orig_spec = *renderer.image_specification();
    let max_preview = 1280;
    let preview_spec = scale_preview(&orig_spec, max_preview, max_preview);
    let [pw, ph] = preview_spec.resolution;

    eprintln!(
        "\nColor Map Editor Controls:\n\
         \x20 • Click/drag markers on the timeline to reposition keyframes\n\
         \x20 • ←/→ arrow keys     – cycle through keyframes\n\
         \x20 • Shift+←/→          – nudge query 10 % toward neighbor\n\
         \x20 • Click any value box – type a new number, press Enter to apply\n\
         \x20 • Enter (no input)    – trigger full-quality render\n\
         \x20 • N / Return          – add keyframe at midpoint\n\
         \x20 • Delete/Backspace    – remove selected keyframe\n\
         \x20 • S or Space          – save params\n\
         \x20 • Escape              – save and quit\n"
    );

    let event_loop = EventLoop::new();

    let fractal_window = WindowBuilder::new()
        .with_title("Color Map Editor – Fractal Preview")
        .with_inner_size(LogicalSize::new(pw as f64, ph as f64))
        .with_resizable(false)
        .build(&event_loop).unwrap();

    let editor_window = WindowBuilder::new()
        .with_title("Color Map Editor – Controls")
        .with_inner_size(LogicalSize::new(W as f64, H as f64))
        .with_resizable(false)
        .build(&event_loop).unwrap();

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

    // Capture quality baseline ONCE before any speed adjustments.
    let speed_cache = renderer.reference_cache();

    // Start with a fast preview render.
    renderer.set_speed_optimization_level(FAST_LEVEL, &speed_cache);

    let renderer      = Arc::new(Mutex::new(renderer));
    let display_buffer: Arc<Mutex<Vec<Vec<image::Rgb<u8>>>>> =
        Arc::new(Mutex::new(create_buffer(image::Rgb([0u8,0,0]), &[pw, ph])));
    let render_busy  = Arc::new(AtomicBool::new(false));
    let render_ready = Arc::new(AtomicBool::new(false));

    let mut state = EditorState::new(initial_keyframes);

    spawn_render(renderer.clone(), display_buffer.clone(), render_busy.clone(), render_ready.clone());

    event_loop.run(move |event, _, control_flow| {
        if let Event::NewEvents(StartCause::Init) = event {
            *control_flow = ControlFlow::Wait;
        }

        // ── Window events ──────────────────────────────────────────────────
        if let Event::WindowEvent { window_id, ref event } = event {
            match event {
                WindowEvent::CloseRequested => {
                    save_fn(&state.keyframes);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                // Keyboard events work from either window.
                WindowEvent::ReceivedCharacter(_)
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::ModifiersChanged(_) => {
                    if state.mode != Mode::Rendering || matches!(event, WindowEvent::KeyboardInput { .. }) {
                        state.handle_window_event(event);
                    }
                }
                WindowEvent::Resized(sz) if window_id == fractal_wid => {
                    fractal_pixels.resize_surface(sz.width, sz.height).ok();
                }
                WindowEvent::Resized(sz) if window_id == editor_wid => {
                    editor_pixels.resize_surface(sz.width, sz.height).ok();
                }
                _ if window_id == editor_wid => {
                    if state.mode != Mode::Rendering {
                        state.handle_window_event(event);
                    }
                }
                _ => {}
            }
        }

        // ── Redraws ────────────────────────────────────────────────────────
        if let Event::RedrawRequested(wid) = event {
            if wid == fractal_wid {
                draw_preview(fractal_pixels.frame_mut(), &display_buffer.lock().unwrap(), pw, ph);
                fractal_pixels.render().ok();
            } else if wid == editor_wid {
                draw_editor(editor_pixels.frame_mut(), &state);
                editor_pixels.render().ok();
            }
        }

        // ── Main tick ──────────────────────────────────────────────────────
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

            // Enter outside text input → full quality render.
            if state.full_render_requested {
                state.full_render_requested = false;
                state.mode = Mode::Rendering;
                state.render_start = Some(Instant::now());
                state.keyframes_dirty = true; // force a re-render at full quality
                state.view_dirty = true;
            }

            let need_editor = state.view_dirty || state.keyframes_dirty
                || state.mode == Mode::Rendering;
            state.view_dirty = false;

            if state.keyframes_dirty {
                // Update the renderer's color map.
                renderer.lock().unwrap().set_keyframes(state.keyframes.clone());

                // Choose quality based on mode.
                let level = if state.mode == Mode::Rendering { FULL_LEVEL } else { FAST_LEVEL };
                renderer.lock().unwrap().set_speed_optimization_level(level, &speed_cache);

                if spawn_render(renderer.clone(), display_buffer.clone(),
                                render_busy.clone(), render_ready.clone()) {
                    state.keyframes_dirty = false;
                    if state.mode == Mode::Rendering {
                        state.render_start = Some(Instant::now());
                    }
                }
            }

            if render_ready.swap(false, Ordering::AcqRel) {
                fractal_window.request_redraw();
                // If a full render just completed, return to color mode.
                if state.mode == Mode::Rendering {
                    state.mode = Mode::Color;
                    state.render_start = None;
                    state.view_dirty = true;
                }
            }
            if need_editor { editor_window.request_redraw(); }

            let active = render_busy.load(Ordering::Acquire)
                || state.mouse_down
                || state.mode == Mode::Rendering;
            *control_flow = if active {
                ControlFlow::WaitUntil(Instant::now() + std::time::Duration::from_millis(16))
            } else {
                ControlFlow::Wait
            };
        }
    });
}
