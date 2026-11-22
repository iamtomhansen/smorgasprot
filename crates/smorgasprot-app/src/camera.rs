//! Orbit camera: maps plane-local 3D coordinates to screen pixels.
//! Owns orbit/pan/zoom/roll state, pointer handling, and axis-aligned view snapping.

use crate::state::NavigationParams;
use egui::{PointerButton, Pos2, Rect, Response, Ui, Vec2};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

/// Default zoom in screen pixels per Ångström.
pub const DEFAULT_ZOOM: f32 = 14.5;

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
            zoom: DEFAULT_ZOOM,
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
