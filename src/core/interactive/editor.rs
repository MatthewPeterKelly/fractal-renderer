//! Live color-palette editor panel for the interactive explorer.
//!
//! Renders a `ColorPalette` as an editable widget: a background swatch, one
//! tab per color map (suppressed for the single-map case), a vertical list of
//! per-keyframe color cells with insert (`+`) buttons and segment-fraction
//! drag values between them, a read-only gradient bar, and an inline color
//! picker bound to the selected keyframe. The widget mutates the palette in
//! place and reports whether anything changed this frame so the caller can
//! trigger a color-only re-render.

use egui::{Color32, Sense};

use crate::core::color_map::{
    ColorMap, ColorMapKeyFrame, ColorMapper, ColorPalette, KeyframeColorMap,
};
use crate::core::interpolation::LinearInterpolator;

/// Smallest segment fraction allowed, so no two adjacent keyframes collapse
/// onto the same query position.
const MIN_FRACTION: f32 = 0.001;

/// Height of each keyframe color cell, in logical pixels.
const CELL_HEIGHT: f32 = 28.0;

/// Height of the read-only gradient bar, in logical pixels.
const GRADIENT_BAR_HEIGHT: f32 = 40.0;

/// Persistent editor selection state, owned by the interactive app and
/// threaded into [`show_palette_editor`] each frame.
#[derive(Debug, Default, Clone)]
pub struct EditorState {
    /// Index of the selected keyframe within the active color map, if any.
    pub selected_keyframe: Option<usize>,
    /// Index of the color map currently being edited (the active tab).
    pub active_color_map: usize,
}

/// Render the editor for `palette`, mutating it in place. Returns `true` if
/// any keyframe color, segment fraction, or the background color changed this
/// frame (the caller should then mark the preview dirty).
pub fn show_palette_editor(
    palette: &mut ColorPalette,
    ui: &mut egui::Ui,
    state: &mut EditorState,
) -> bool {
    let mut changed = false;

    ui.heading("Color Map");
    ui.separator();

    // Background color (used for `None` cells: in-set / out-of-basin).
    ui.horizontal(|ui| {
        ui.label("Background:");
        let mut color = Color32::from_rgb(
            palette.background_color[0],
            palette.background_color[1],
            palette.background_color[2],
        );
        if ui.color_edit_button_srgba(&mut color).changed() {
            palette.background_color = [color.r(), color.g(), color.b()];
            changed = true;
        }
    });
    ui.separator();

    // Tab strip: one tab per color map. Suppressed when there is only one.
    let map_count = palette.color_maps.len();
    if map_count > 1 {
        ui.horizontal_wrapped(|ui| {
            for idx in 0..map_count {
                if ui
                    .selectable_label(state.active_color_map == idx, format!("Root {idx}"))
                    .clicked()
                    && state.active_color_map != idx
                {
                    state.active_color_map = idx;
                    state.selected_keyframe = None;
                }
            }
        });
        ui.separator();
    }
    if state.active_color_map >= map_count {
        state.active_color_map = 0;
        state.selected_keyframe = None;
    }
    let active = state.active_color_map;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label("Keyframes:");

        // Defer structural edits until after the read-only borrow above is
        // released. At most one of these fires per frame in practice.
        let mut fraction_edit: Option<(usize, f32)> = None;
        let mut insert_after: Option<usize> = None;

        let keyframe_count = palette.color_maps[active].len();
        for i in 0..keyframe_count {
            let keyframe = palette.color_maps[active][i];
            let cell_width = ui.available_width().min(220.0);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(cell_width, CELL_HEIGHT), Sense::click());
            let color = Color32::from_rgb(
                keyframe.rgb_raw[0],
                keyframe.rgb_raw[1],
                keyframe.rgb_raw[2],
            );
            // Draw a white outline for the selected cell by filling the full
            // rect white and insetting the color — avoids stroke sub-pixel
            // artifacts and needs no extra API surface.
            if state.selected_keyframe == Some(i) {
                ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
                ui.painter().rect_filled(rect.shrink(2.0), 2.0, color);
            } else {
                ui.painter().rect_filled(rect, 2.0, color);
            }
            if response.clicked() {
                state.selected_keyframe = Some(i);
            }

            // Between this keyframe and the next: insert button + the
            // fraction of the gradient that this segment occupies.
            if i + 1 < keyframe_count {
                let fraction = palette.color_maps[active][i + 1].query - keyframe.query;
                ui.horizontal(|ui| {
                    if ui.small_button("+").clicked() {
                        insert_after = Some(i);
                    }
                    let mut value = fraction;
                    if ui
                        .add(
                            egui::DragValue::new(&mut value)
                                .speed(0.005)
                                .range(0.0..=1.0),
                        )
                        .changed()
                    {
                        fraction_edit = Some((i, value));
                    }
                });
            }
        }

        if let Some((segment, value)) = fraction_edit {
            set_segment_fraction(&mut palette.color_maps[active], segment, value);
            changed = true;
        }
        if let Some(segment) = insert_after {
            insert_midpoint(&mut palette.color_maps[active], segment);
            changed = true;
        }

        ui.separator();
        ui.label("Gradient:");
        let bar_width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(bar_width, GRADIENT_BAR_HEIGHT), Sense::hover());
        paint_gradient_bar(ui.painter(), rect, &palette.color_maps[active]);

        // Inline color picker bound to the selected keyframe.
        ui.separator();
        if let Some(selected) = state.selected_keyframe {
            if selected < palette.color_maps[active].len() {
                let keyframe = &mut palette.color_maps[active][selected];
                let mut color = Color32::from_rgb(
                    keyframe.rgb_raw[0],
                    keyframe.rgb_raw[1],
                    keyframe.rgb_raw[2],
                );
                if egui::color_picker::color_picker_color32(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                ) {
                    keyframe.rgb_raw = [color.r(), color.g(), color.b()];
                    changed = true;
                }
            } else {
                state.selected_keyframe = None;
            }
        } else {
            ui.label("Click a keyframe to edit its color.");
        }
    });

    changed
}

/// Set the fraction of the gradient occupied by the segment between keyframe
/// `segment` and `segment + 1`, scaling the other segments proportionally so
/// the fractions still sum to 1.0, then recomputing keyframe positions. The
/// edited fraction is clamped so every segment keeps at least `MIN_FRACTION`
/// width (no collapse to zero).
pub fn set_segment_fraction(map: &mut ColorMap, segment: usize, new_value: f32) {
    let keyframe_count = map.len();
    if keyframe_count < 2 || segment + 1 >= keyframe_count {
        return;
    }
    let mut fractions: Vec<f32> = (0..keyframe_count - 1)
        .map(|i| map[i + 1].query - map[i].query)
        .collect();
    let other_count = fractions.len() - 1;
    // Leave room for every other segment to retain at least MIN_FRACTION.
    let max_value = (1.0 - MIN_FRACTION * other_count as f32).max(MIN_FRACTION);
    let new_value = new_value.clamp(MIN_FRACTION, max_value);
    let remaining = 1.0 - new_value;
    let old_others_sum: f32 = fractions
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != segment)
        .map(|(_, f)| *f)
        .sum();
    for (i, fraction) in fractions.iter_mut().enumerate() {
        if i == segment {
            *fraction = new_value;
        } else if old_others_sum > 0.0 {
            *fraction = (*fraction * remaining / old_others_sum).max(MIN_FRACTION);
        } else {
            *fraction = remaining / other_count as f32;
        }
    }
    // Normalize away any drift introduced by the MIN_FRACTION clamps, then
    // rebuild positions from the cumulative sum. Anchors stay at 0.0 / 1.0.
    let total: f32 = fractions.iter().sum();
    let mut query = 0.0;
    for i in 0..keyframe_count - 1 {
        map[i].query = query;
        query += fractions[i] / total;
    }
    map[keyframe_count - 1].query = 1.0;
}

/// Insert a new keyframe at the midpoint of the segment between keyframe
/// `segment` and `segment + 1`. Its color is the linear interpolation of the
/// two neighbors, so the insertion is initially invisible in the gradient.
pub fn insert_midpoint(map: &mut ColorMap, segment: usize) {
    if segment + 1 >= map.len() {
        return;
    }
    let lower = map[segment];
    let upper = map[segment + 1];
    let midpoint_channel = |a: u8, b: u8| (((a as f32) + (b as f32)) * 0.5).round() as u8;
    map.insert(
        segment + 1,
        ColorMapKeyFrame {
            query: 0.5 * (lower.query + upper.query),
            rgb_raw: [
                midpoint_channel(lower.rgb_raw[0], upper.rgb_raw[0]),
                midpoint_channel(lower.rgb_raw[1], upper.rgb_raw[1]),
                midpoint_channel(lower.rgb_raw[2], upper.rgb_raw[2]),
            ],
        },
    );
}

/// Remove the keyframe at `index`. The first and last keyframes are the
/// 0.0 / 1.0 anchors and cannot be removed; returns `true` if a keyframe was
/// actually removed.
pub fn delete_keyframe(map: &mut ColorMap, index: usize) -> bool {
    if index == 0 || index + 1 >= map.len() {
        return false;
    }
    map.remove(index);
    true
}

/// Paint a gradient bar showing the color map as contiguous filled
/// rectangles. Lifted from the (soon-to-be-deleted) demo color editor: at
/// fractional DPI, 1-logical-pixel strokes anti-alias across two physical
/// pixels and leave visible gaps; adjacent rects avoid that artifact.
fn paint_gradient_bar(painter: &egui::Painter, rect: egui::Rect, keyframes: &[ColorMapKeyFrame]) {
    if keyframes.is_empty() {
        return;
    }
    let color_map = KeyframeColorMap::new(keyframes, LinearInterpolator {});
    let column_count = (rect.width() as u32).max(2);
    let t_step = 1.0 / (column_count - 1) as f32;
    let step_w = rect.width() / column_count as f32;
    for i in 0..column_count {
        let t = i as f32 * t_step;
        let rgb = color_map.compute_pixel(t);
        let x0 = rect.left() + i as f32 * step_w;
        let x1 = rect.left() + (i + 1) as f32 * step_w;
        painter.rect_filled(
            egui::Rect::from_x_y_ranges(x0..=x1, rect.top()..=rect.bottom()),
            0.0,
            Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_with_queries(queries: &[f32]) -> ColorMap {
        queries
            .iter()
            .map(|&query| ColorMapKeyFrame {
                query,
                rgb_raw: [0, 0, 0],
            })
            .collect()
    }

    fn queries(map: &ColorMap) -> Vec<f32> {
        map.iter().map(|kf| kf.query).collect()
    }

    #[test]
    fn set_fraction_scales_others_proportionally() {
        // Four evenly-spaced keyframes → three 1/3 segments.
        let mut map = map_with_queries(&[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]);
        set_segment_fraction(&mut map, 0, 0.5);
        let q = queries(&map);
        // First segment is now 0.5; the remaining 0.5 splits evenly between
        // the two equal trailing segments → 0.25 each.
        assert!((q[0] - 0.0).abs() < 1e-5);
        assert!((q[1] - 0.5).abs() < 1e-5);
        assert!((q[2] - 0.75).abs() < 1e-5);
        assert!((q[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn set_fraction_clamps_to_min_and_max() {
        let mut map = map_with_queries(&[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]);
        // Editing to 0 clamps up to MIN_FRACTION.
        set_segment_fraction(&mut map, 0, 0.0);
        let first_segment = map[1].query - map[0].query;
        assert!(first_segment >= MIN_FRACTION - 1e-6);
        // Editing to 1.0 clamps down so the other two segments keep room.
        set_segment_fraction(&mut map, 0, 1.0);
        let first_segment = map[1].query - map[0].query;
        assert!(first_segment <= 1.0 - 2.0 * MIN_FRACTION + 1e-6);
        // Anchors are preserved and order stays monotonic.
        assert_eq!(map[0].query, 0.0);
        assert_eq!(map[3].query, 1.0);
        assert!(map[0].query < map[1].query);
        assert!(map[1].query < map[2].query);
        assert!(map[2].query < map[3].query);
    }

    #[test]
    fn insert_midpoint_uses_interpolated_color_and_position() {
        let mut map = vec![
            ColorMapKeyFrame {
                query: 0.0,
                rgb_raw: [0, 0, 0],
            },
            ColorMapKeyFrame {
                query: 1.0,
                rgb_raw: [100, 200, 50],
            },
        ];
        insert_midpoint(&mut map, 0);
        assert_eq!(map.len(), 3);
        assert!((map[1].query - 0.5).abs() < 1e-6);
        assert_eq!(map[1].rgb_raw, [50, 100, 25]);
    }

    #[test]
    fn delete_keyframe_drops_middle_and_keeps_anchors() {
        let mut map = map_with_queries(&[0.0, 0.5, 1.0]);
        assert!(delete_keyframe(&mut map, 1));
        assert_eq!(queries(&map), vec![0.0, 1.0]);
    }

    #[test]
    fn delete_keyframe_refuses_anchors() {
        let mut map = map_with_queries(&[0.0, 0.5, 1.0]);
        assert!(!delete_keyframe(&mut map, 0));
        assert!(!delete_keyframe(&mut map, 2));
        assert_eq!(map.len(), 3);
    }
}
