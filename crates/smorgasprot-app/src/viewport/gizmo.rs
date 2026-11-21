//! 3D orientation gizmo with depth-sorted u, v, n coordinate axes.

use crate::camera::CameraState;
use egui::{pos2, Align2, Color32, FontId, Painter, Rect, Stroke};

/// Draws the interactive 3D orientation gizmo in the top-right corner of the viewport.
pub fn draw_orientation_gizmo(painter: &Painter, rect: Rect, camera: &CameraState) {
    let cx = rect.max.x - 56.0;
    let cy = rect.min.y + 56.0;
    let arm_len = 34.0;
    let center = pos2(cx, cy);

    painter.circle(center, 44.0, Color32::from_rgba_unmultiplied(15, 23, 42, 198), Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(148, 163, 184, 90)));

    let axes = [
        ("u",  [ 1.0_f32,  0.0,  0.0], Color32::from_rgb(239, 68, 68),  true),
        ("-u", [-1.0_f32,  0.0,  0.0], Color32::from_rgb(153, 27, 27),  false),
        ("v",  [ 0.0_f32,  1.0,  0.0], Color32::from_rgb(34, 197, 94),  true),
        ("-v", [ 0.0_f32, -1.0,  0.0], Color32::from_rgb(22, 101, 52),  false),
        ("n",  [ 0.0_f32,  0.0,  1.0], Color32::from_rgb(59, 130, 246), true),
        ("-n", [ 0.0_f32,  0.0, -1.0], Color32::from_rgb(30, 64, 175),  false),
    ];

    let mut projected: Vec<_> = axes.iter().map(|&(lbl, dir, col, pos)| {
        let r = camera.rotate_axis_for_gizmo(dir);
        let scr = pos2(cx + r[0] * arm_len, cy - r[1] * arm_len);
        (lbl, scr, r[2], col, pos)
    }).collect();

    projected.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    for (lbl, pt, depth, col, pos) in projected {
        if pos {
            painter.line_segment([center, pt], Stroke::new(2.2_f32, col));
        }
        let rad = if pos { 8.5 } else { 6.0 };
        let alpha = if depth >= -0.1 { 255 } else { 150 };
        let fill = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha);
        painter.circle(pt, rad, fill, Stroke::new(1.2_f32, Color32::from_rgb(15, 23, 42)));
        if pos {
            painter.text(pt, Align2::CENTER_CENTER, lbl, FontId::proportional(10.0), Color32::WHITE);
        }
    }
}
