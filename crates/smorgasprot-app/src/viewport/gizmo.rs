//! Orientation gizmo in the viewport's top-right corner.
//! Shows the camera-rotated `u, v, n` axes; clicking a cap snaps the view to that axis.

use crate::camera::{CameraState, ViewAxis};
use crate::style::palette;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, pos2};

/// Distance from the viewport's top-right corner to the gizmo center.
const MARGIN: f32 = 56.0;
/// Screen length of a unit axis.
const ARM: f32 = 34.0;
/// Radius of the dark background disc.
const BG_RADIUS: f32 = 44.0;
/// Cap radii for positive and negative axes.
const CAP_RADIUS_POS: f32 = 8.5;
const CAP_RADIUS_NEG: f32 = 6.0;
// Premultiplied slate-900 at 78 % and slate-400 at 35 % (const-friendly).
const BG_FILL: Color32 = Color32::from_rgba_premultiplied(12, 18, 33, 198);
const BG_RIM: Color32 = Color32::from_rgba_premultiplied(52, 57, 65, 90);

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

fn axis_color(axis: ViewAxis) -> Color32 {
    let (pos, neg) = match axis {
        ViewAxis::PosU | ViewAxis::NegU => palette::AXIS_U,
        ViewAxis::PosV | ViewAxis::NegV => palette::AXIS_V,
        ViewAxis::PosN | ViewAxis::NegN => palette::AXIS_N,
    };
    if axis.is_positive() { pos } else { neg }
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

/// True if `pointer` lies on the gizmo background (used to suppress scene picking).
pub fn contains(rect: Rect, pointer: Pos2) -> bool {
    center(rect).distance(pointer) <= BG_RADIUS
}

/// Returns the nearest cap under `pointer`, if any.
pub fn hit_test(rect: Rect, camera: &CameraState, pointer: Pos2) -> Option<ViewAxis> {
    // Iterate near-to-far so the front-most cap wins on overlap.
    caps(rect, camera)
        .into_iter()
        .rev()
        .find(|cap| cap.pos.distance(pointer) <= cap_radius(cap.axis) + 2.0)
        .map(|cap| cap.axis)
}

/// Paints the gizmo; `hovered` gets a highlight ring.
pub fn paint(painter: &Painter, rect: Rect, camera: &CameraState, hovered: Option<ViewAxis>) {
    let c = center(rect);
    painter.circle(c, BG_RADIUS, BG_FILL, Stroke::new(1.0_f32, BG_RIM));

    for cap in caps(rect, camera) {
        let color = axis_color(cap.axis);
        let positive = cap.axis.is_positive();
        if positive {
            painter.line_segment([c, cap.pos], Stroke::new(2.2_f32, color));
        }
        // Caps pointing away from the viewer are dimmed.
        let fill = if cap.depth >= -0.1 { color } else { palette::with_alpha(color, 0.6) };
        let outline = if hovered == Some(cap.axis) { Stroke::new(2.0_f32, Color32::WHITE) } else { Stroke::new(1.2_f32, palette::INK) };
        painter.circle(cap.pos, cap_radius(cap.axis), fill, outline);
        if positive {
            painter.text(cap.pos, Align2::CENTER_CENTER, cap.axis.label(), FontId::proportional(10.0), Color32::WHITE);
        }
    }
}
