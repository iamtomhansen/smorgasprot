//! Reference grid drawn on the interface tangent plane (plane-local `n = 0`).

use crate::camera::CameraState;
use crate::style::palette;
use egui::{Painter, Rect, Stroke};
use glam::Vec3;

/// Half-width of the grid in Ångström.
const EXTENT_A: i32 = 40;
/// Grid line spacing in Ångström.
const STEP_A: usize = 5;

/// Paints the tangent-plane grid underneath all scene geometry.
pub fn paint(painter: &Painter, rect: Rect, camera: &CameraState) {
    let stroke = Stroke::new(1.0_f32, palette::GRID);
    let e = EXTENT_A as f32;
    let line = |a: Vec3, b: Vec3| {
        painter.line_segment([camera.project(a, rect).0, camera.project(b, rect).0], stroke);
    };
    for g in (-EXTENT_A..=EXTENT_A).step_by(STEP_A) {
        let g = g as f32;
        // One line parallel to v, one parallel to u.
        line(Vec3::new(g, -e, 0.0), Vec3::new(g, e, 0.0));
        line(Vec3::new(-e, g, 0.0), Vec3::new(e, g, 0.0));
    }
}
