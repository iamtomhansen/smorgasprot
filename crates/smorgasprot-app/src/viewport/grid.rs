//! Reference grid drawn on the interface tangent plane (plane-local `n = 0`).

use crate::camera::CameraState;
use crate::state::GridParams;
use crate::style::palette;
use egui::{Painter, Rect, Stroke};
use glam::Vec3;

/// Paints the tangent-plane grid underneath all scene geometry.
pub fn paint(painter: &Painter, rect: Rect, camera: &CameraState, grid: &GridParams) {
    let stroke = Stroke::new(1.0_f32, palette::GRID);
    let extent = grid.grid_extent_a;
    let step = grid.grid_step_a.max(0.5);
    let line = |a: Vec3, b: Vec3| {
        painter.line_segment([camera.project(a, rect).0, camera.project(b, rect).0], stroke);
    };
    let mut g = -extent;
    while g <= extent + 1e-4 {
        // One line parallel to v, one parallel to u.
        line(Vec3::new(g, -extent, 0.0), Vec3::new(g, extent, 0.0));
        line(Vec3::new(-extent, g, 0.0), Vec3::new(extent, g, 0.0));
        g += step;
    }
}
