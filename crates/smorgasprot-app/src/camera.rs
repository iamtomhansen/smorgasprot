//! Orbit camera: maps plane-local 3D coordinates to screen pixels.
//! Owns orbit/pan/zoom/roll state, pointer handling, and axis-aligned view snapping.

use crate::state::NavigationParams;
use egui::{PointerButton, Pos2, Rect, Response, Ui, Vec2};
use glam::{Mat3, Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

/// One of the six signed plane axes `(±u, ±v, ±n)`, used for gizmo caps and view snapping.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ViewAxis {
    PosU,
    NegU,
    PosV,
    NegV,
    PosN,
    NegN,
}

impl ViewAxis {
    /// All six axes in display order, for generic editors.
    pub const ALL: [Self; 6] = [Self::PosU, Self::NegU, Self::PosV, Self::NegV, Self::PosN, Self::NegN];

    /// Unit direction of this axis in plane-local space.
    pub fn dir(self) -> Vec3 {
        match self {
            Self::PosU => Vec3::X,
            Self::NegU => Vec3::NEG_X,
            Self::PosV => Vec3::Y,
            Self::NegV => Vec3::NEG_Y,
            Self::PosN => Vec3::Z,
            Self::NegN => Vec3::NEG_Z,
        }
    }

    /// Short label drawn on the gizmo cap.
    pub fn label(self) -> &'static str {
        match self {
            Self::PosU => "u",
            Self::NegU => "-u",
            Self::PosV => "v",
            Self::NegV => "-v",
            Self::PosN => "n",
            Self::NegN => "-n",
        }
    }

    /// Returns true if this axis points toward the positive half-space of the coordinate system.
    pub fn is_positive(self) -> bool {
        matches!(self, Self::PosU | Self::PosV | Self::PosN)
    }
}

/// Yaw/pitch/roll orbit camera with 3D origin, screen-space pan, and orthographic zoom.
/// Plane-local axes at rest: `u` right, `v` up, `n` toward the viewer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct CameraState {
    /// Rotation about the screen-vertical axis (radians).
    pub yaw: f32,
    /// Rotation about the screen-horizontal axis (radians), clamped to ±π/2.
    pub pitch: f32,
    /// Rotation about the line of sight (radians).
    pub roll: f32,
    /// 3D focal point / orbit origin in plane-local space (Å).
    pub origin: Vec3,
    /// Screen-space pan offset in pixels.
    pub pan: Vec2,
    /// Scale in screen pixels per Ångström.
    pub zoom: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            origin: Vec3::ZERO,
            pan: Vec2::ZERO,
            zoom: NavigationParams::default().default_zoom,
        }
    }
}

impl CameraState {
    /// Rotates a plane-local vector into view space (`z` toward the viewer).
    pub fn rotate(&self, p: Vec3) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // Yaw about v, then pitch about the rotated u.
        let x1 = p.x * cy + p.z * sy;
        let z1 = -p.x * sy + p.z * cy;
        let x2 = x1;
        let y2 = p.y * cp - z1 * sp;
        let z2 = p.y * sp + z1 * cp;
        // If roll is applied, rotate about the rotated n.
        if self.roll != 0.0 {
            let (sr, cr) = self.roll.sin_cos();
            Vec3::new(x2 * cr - y2 * sr, x2 * sr + y2 * cr, z2)
        } else {
            Vec3::new(x2, y2, z2)
        }
    }

    /// Projects a plane-local point to `(screen position, depth)`; larger depth is nearer.
    pub fn project(&self, local: Vec3, rect: Rect) -> (Pos2, f32) {
        let rel = local - self.origin;
        let r = self.rotate(rel);
        let c = rect.center() + self.pan;
        (Pos2::new(c.x + r.x * self.zoom, c.y - r.y * self.zoom), r.z)
    }

    /// GPU-pipeline equivalent of `rotate()` + `project()`'s orthographic pixel mapping,
    /// for shaders that need a real matrix (the GPU molecular-surface renderer; later,
    /// screen-ray unprojection for 3D picking). Produces wgpu clip space: x right, y up,
    /// depth `0..1` with **smaller = nearer** — the opposite sign of `project()`'s own
    /// "larger local-z is nearer" depth, since that's wgpu's convention, not ours.
    ///
    /// `half_depth_a` is a symmetric view-space depth half-range (Å) around `origin`
    /// along the view axis; it must bound every point that will be rendered (otherwise
    /// it's clipped), but otherwise doesn't affect the on-screen result — this is an
    /// orthographic projection, exactly like `project()`, which never divides by depth.
    pub fn view_projection_matrix(&self, rect: Rect, half_depth_a: f32) -> Mat4 {
        // Same rotation composition as `rotate()`, in the same order, so a GPU-rendered
        // layer lines up exactly with every CPU-projected one sharing this camera.
        let rotation = Mat3::from_rotation_z(self.roll) * Mat3::from_rotation_x(self.pitch) * Mat3::from_rotation_y(self.yaw);
        let view = Mat4::from_mat3(rotation) * Mat4::from_translation(-self.origin);

        // Orthographic projection matching `project()`'s `center + pan ± r * zoom` pixel
        // mapping, inverted through the standard NDC→viewport transform (NDC y up maps
        // to the top of the viewport, matching `project()`'s screen-y-down convention).
        let (w, h) = (rect.width(), rect.height());
        let proj = Mat4::from_cols(
            glam::Vec4::new(2.0 * self.zoom / w, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, 2.0 * self.zoom / h, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, -0.5 / half_depth_a, 0.0),
            glam::Vec4::new(2.0 * self.pan.x / w, -2.0 * self.pan.y / h, 0.5, 1.0),
        );
        proj * view
    }

    /// Restores the default face-on view of the tangent plane.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Orients the camera so `axis` points straight at the viewer and resets roll.
    pub fn snap_to(&mut self, axis: ViewAxis) {
        let (yaw, pitch) = match axis {
            ViewAxis::PosN => (0.0, 0.0),
            ViewAxis::NegN => (PI, 0.0),
            ViewAxis::PosU => (-FRAC_PI_2, 0.0),
            ViewAxis::NegU => (FRAC_PI_2, 0.0),
            ViewAxis::PosV => (0.0, FRAC_PI_2),
            ViewAxis::NegV => (0.0, -FRAC_PI_2),
        };
        self.yaw = yaw;
        self.pitch = pitch;
        self.roll = 0.0;
    }

    /// Applies drag/scroll/pinch input from the viewport `response`.
    /// `primary_pans` makes left-drag pan instead of orbit (used for the flat 2D map).
    pub fn handle_input(&mut self, ui: &Ui, response: &Response, primary_pans: bool, nav: &NavigationParams) {
        let delta = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);

        // Left drag orbits (or pans); right/middle drag always pans.
        if response.dragged_by(PointerButton::Primary) {
            if shift || primary_pans {
                self.pan += delta;
            } else {
                self.yaw += delta.x * nav.orbit_rad_per_px;
                self.pitch = (self.pitch + delta.y * nav.orbit_rad_per_px).clamp(-FRAC_PI_2, FRAC_PI_2);
            }
        } else if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
            self.pan += delta;
        }

        // Zoom only when the pointer is over the viewport, so side-panel scrolling is unaffected.
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = pinch * (scroll * nav.zoom_per_scroll).exp();
            self.zoom = (self.zoom * factor).clamp(nav.min_zoom, nav.max_zoom);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `view_projection_matrix` must land on the exact same screen pixel (and agree on
    /// relative depth order) as `project()` for every point, across a spread of camera
    /// states. Ensures GPU and CPU rendering are mathematically identical.
    #[test]
    fn matrix_matches_pointwise_projection() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(800.0, 600.0));
        let half_depth_a = 1000.0;

        let cameras = [
            CameraState::default(),
            CameraState { yaw: 0.6, ..CameraState::default() },
            CameraState { pitch: -0.4, ..CameraState::default() },
            CameraState { yaw: 0.3, pitch: 0.5, roll: 0.0, ..CameraState::default() },
            CameraState { yaw: 0.3, pitch: 0.5, roll: 0.9, origin: Vec3::new(5.0, -3.0, 2.0), pan: Vec2::new(40.0, -15.0), zoom: 12.0 },
            CameraState { yaw: -1.2, pitch: FRAC_PI_2 * 0.9, roll: -0.5, zoom: 3.5, ..CameraState::default() },
        ];
        let points = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(3.5, -2.0, 7.0),
            Vec3::new(-12.0, 6.0, -4.0),
        ];

        for cam in cameras {
            let vp = cam.view_projection_matrix(rect, half_depth_a);
            let mut prev_depth = None;
            for &p in &points {
                let (expected_screen, depth) = cam.project(p, rect);

                let clip = vp * p.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                let got_screen = Pos2::new(
                    rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(),
                    rect.top() + (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height(),
                );

                assert!(
                    (got_screen - expected_screen).length() < 1e-3,
                    "screen mismatch: matrix={got_screen:?} project()={expected_screen:?} cam={cam:?} p={p:?}"
                );
                // wgpu convention is smaller-depth-nearer, the opposite of `project()`'s
                // larger-is-nearer, so the ranking across points must invert.
                if let Some((prev_ndc_z, prev_local_depth)) = prev_depth {
                    assert_eq!(
                        prev_ndc_z < ndc.z,
                        prev_local_depth > depth,
                        "depth order mismatch at cam={cam:?}"
                    );
                }
                prev_depth = Some((ndc.z, depth));
            }
        }
    }
}
