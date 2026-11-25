//! Reference grid drawn on the interface tangent plane (plane-local `n = 0`).

use crate::camera::CameraState;
use crate::state::GridParams;
use crate::style::palette;
use egui::{Color32, Painter, Rect, Stroke};
use glam::Vec3;

/// Iterates over every `(a, b)` line segment of the tangent-plane grid in plane-local space (`n = 0`).
pub fn for_each_segment(grid: &GridParams, mut emit: impl FnMut(Vec3, Vec3)) {
    let extent = grid.grid_extent_a;
    let step = grid.grid_step_a.max(0.5);
    let mut g = -extent;
    while g <= extent + 1e-4 {
        // One line parallel to v, one parallel to u.
        emit(Vec3::new(g, -extent, 0.0), Vec3::new(g, extent, 0.0));
        emit(Vec3::new(-extent, g, 0.0), Vec3::new(extent, g, 0.0));
        g += step;
    }
}

/// Paints the tangent-plane grid underneath all scene geometry (CPU fallback & Cartography).
pub fn paint(painter: &Painter, rect: Rect, camera: &CameraState, grid: &GridParams, color: Color32) {
    if grid.alpha <= 0.004 || grid.grid_extent_a <= 0.0 {
        return;
    }
    let stroke = Stroke::new(1.0_f32, palette::with_alpha(color, grid.alpha));
    for_each_segment(grid, |a, b| {
        painter.line_segment([camera.project(a, rect).0, camera.project(b, rect).0], stroke);
    });
}
