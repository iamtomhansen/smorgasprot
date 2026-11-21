//! Coordinate reference grid drawn on the interface tangent plane (z = 0).

use crate::camera::CameraState;
use egui::{Color32, Painter, Rect, Stroke};

/// Draws a subtle reference grid aligned to the tangent plane Π at z = 0.
pub fn draw_tangent_grid(painter: &Painter, rect: Rect, camera: &CameraState) {
    let grid_stroke = Stroke::new(1.0_f32, Color32::from_rgb(226, 232, 240));
    for g in (-40..=40).step_by(5) {
        let g = g as f32;
        let (p1, _) = camera.project_local_point([g, -40.0, 0.0], rect);
        let (p2, _) = camera.project_local_point([g, 40.0, 0.0], rect);
        painter.line_segment([p1, p2], grid_stroke);

        let (q1, _) = camera.project_local_point([-40.0, g, 0.0], rect);
        let (q2, _) = camera.project_local_point([40.0, g, 0.0], rect);
        painter.line_segment([q1, q2], grid_stroke);
    }
}
