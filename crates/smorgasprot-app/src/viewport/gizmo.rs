//! Orientation gizmo in the viewport's top-right corner.
//! Shows the camera-rotated `u, v, n` axes; clicking a cap snaps the view to that axis.
//! An outer concentric ring controls camera roll around the view axis.

use crate::camera::{CameraState, ViewAxis};
use crate::style::{ChromePalette, GizmoPalette, palette};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, pos2};

/// Distance from the viewport's top-right corner to the gizmo center.
const MARGIN: f32 = 58.0;
/// Screen length of a unit axis.
const ARM: f32 = 34.0;
/// Radius of the dark background disc.
const BG_RADIUS: f32 = 44.0;
/// Roll ring radius and hit range.
const ROLL_RING_RADIUS: f32 = 49.5;
const ROLL_RING_HALF_WIDTH: f32 = 5.0;
/// Maximum interaction radius of the entire gizmo (including roll ring).
const TOTAL_RADIUS: f32 = ROLL_RING_RADIUS + ROLL_RING_HALF_WIDTH;
/// Cap radii for positive and negative axes.
const CAP_RADIUS_POS: f32 = 8.5;
const CAP_RADIUS_NEG: f32 = 6.0;

/// One projected axis cap.
struct Cap {
    axis: ViewAxis,
    pos: Pos2,
    depth: f32,
}

fn center(rect: Rect) -> Pos2 {
    pos2(rect.max.x - MARGIN, rect.min.y + MARGIN)
}

fn cap_radius(axis: ViewAxis) -> f32 {
    if axis.is_positive() { CAP_RADIUS_POS } else { CAP_RADIUS_NEG }
}

/// Projects all six caps, sorted far to near (paint order).
fn caps(rect: Rect, camera: &CameraState) -> Vec<Cap> {
    let c = center(rect);
    let mut caps: Vec<Cap> = ViewAxis::ALL
        .iter()
        .map(|&axis| {
            let r = camera.rotate(axis.dir());
            Cap { axis, pos: pos2(c.x + r.x * ARM, c.y - r.y * ARM), depth: r.z }
        })
        .collect();
    caps.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    caps
}

/// True if `pointer` lies on the gizmo background or roll ring (used to suppress scene picking).
pub fn contains(rect: Rect, pointer: Pos2) -> bool {
    center(rect).distance(pointer) <= TOTAL_RADIUS
}

/// True if `pointer` is in the annular band of the roll ring.
pub fn is_over_roll_ring(rect: Rect, pointer: Pos2) -> bool {
    let d = center(rect).distance(pointer);
    (d - ROLL_RING_RADIUS).abs() <= ROLL_RING_HALF_WIDTH
}

/// Applies roll rotation from a pointer drag.
pub fn handle_roll_drag(rect: Rect, camera: &mut CameraState, pointer: Pos2, delta: egui::Vec2) {
    let c = center(rect);
    let rx = pointer.x - c.x;
    let ry = pointer.y - c.y;
    let r2 = rx * rx + ry * ry;
    if r2 > 1.0 {
        // Tangential angular delta in radians; counter-clockwise roll matches screen orientation.
        let d_theta = -(rx * delta.y - ry * delta.x) / r2;
        camera.roll += d_theta;
        // Snap to zero if within 1 degree
        if camera.roll.abs() < 0.018 {
            camera.roll = 0.0;
        }
    }
}

/// Returns the nearest cap under `pointer`, if any.
pub fn hit_test(rect: Rect, camera: &CameraState, pointer: Pos2) -> Option<ViewAxis> {
    if is_over_roll_ring(rect, pointer) {
        return None;
    }
    // Iterate near-to-far so the front-most cap wins on overlap.
    caps(rect, camera)
        .into_iter()
        .rev()
        .find(|cap| cap.pos.distance(pointer) <= cap_radius(cap.axis) + 2.0)
        .map(|cap| cap.axis)
}

/// Paints the gizmo and outer roll ring; `hovered` gets a highlight outline.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    camera: &CameraState,
    hovered: Option<ViewAxis>,
    over_ring: bool,
    gizmo_pal: &GizmoPalette,
    chrome: &ChromePalette,
) {
    let c = center(rect);
    painter.circle(c, BG_RADIUS, gizmo_pal.bg_fill, Stroke::new(1.0_f32, gizmo_pal.bg_rim));

    // Outer roll ring
    let ring_stroke = if over_ring {
        Stroke::new(2.0_f32, chrome.accent)
    } else {
        Stroke::new(1.2_f32, gizmo_pal.bg_rim)
    };
    painter.circle_stroke(c, ROLL_RING_RADIUS, ring_stroke);

    // 12 o'clock zero tick
    let zero_tick = pos2(c.x, c.y - ROLL_RING_RADIUS);
    let zero_color = if camera.roll.abs() < 0.02 { chrome.accent } else { chrome.muted() };
    painter.circle_filled(zero_tick, 2.0_f32, zero_color);

    // Current roll handle
    let angle = -std::f32::consts::FRAC_PI_2 - camera.roll;
    let (sin_a, cos_a) = angle.sin_cos();
    let handle_pos = pos2(c.x + cos_a * ROLL_RING_RADIUS, c.y + sin_a * ROLL_RING_RADIUS);
    let handle_color = if over_ring { Color32::WHITE } else { chrome.accent };
    painter.circle(handle_pos, 3.5_f32, handle_color, Stroke::new(1.0_f32, chrome.ink));

    // Axis arms and caps
    for cap in caps(rect, camera) {
        let color = gizmo_pal.axis_color(cap.axis);
        let positive = cap.axis.is_positive();
        if positive {
            painter.line_segment([c, cap.pos], Stroke::new(2.2_f32, color));
        }
        // Caps pointing away from the viewer are dimmed.
        let fill = if cap.depth >= -0.1 { color } else { palette::with_alpha(color, 0.6) };
        let outline = if hovered == Some(cap.axis) {
            Stroke::new(2.0_f32, Color32::WHITE)
        } else {
            Stroke::new(1.2_f32, chrome.ink)
        };
        painter.circle(cap.pos, cap_radius(cap.axis), fill, outline);
        if positive {
            painter.text(cap.pos, Align2::CENTER_CENTER, cap.axis.label(), FontId::proportional(10.0), Color32::WHITE);
        }
    }
}
