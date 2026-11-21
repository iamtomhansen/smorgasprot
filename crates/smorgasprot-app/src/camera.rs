//! Orbit camera: maps plane-local 3D coordinates to screen pixels.
//! Owns orbit/pan/zoom state, pointer handling, and axis-aligned view snapping.

use egui::{PointerButton, Pos2, Rect, Response, Ui, Vec2};
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

/// Default zoom in screen pixels per Ångström.
const DEFAULT_ZOOM: f32 = 14.5;
/// Zoom limits in screen pixels per Ångström.
const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 4.0..=75.0;
/// Orbit sensitivity in radians per dragged pixel.
const ORBIT_RAD_PER_PX: f32 = 0.008;
/// Zoom sensitivity per scrolled point (exponential).
const ZOOM_PER_SCROLL: f32 = 0.002;

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

/// Yaw/pitch orbit camera with screen-space pan and orthographic zoom.
/// Plane-local axes at rest: `u` right, `v` up, `n` toward the viewer.
#[derive(Clone, Debug)]
pub struct CameraState {
    /// Rotation about the screen-vertical axis (radians).
    pub yaw: f32,
    /// Rotation about the screen-horizontal axis (radians), clamped to ±π/2.
    pub pitch: f32,
    /// Screen-space pan offset in pixels.
    pub pan: Vec2,
    /// Scale in screen pixels per Ångström.
    pub zoom: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.0, pan: Vec2::ZERO, zoom: DEFAULT_ZOOM }
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
        Vec3::new(x1, p.y * cp - z1 * sp, p.y * sp + z1 * cp)
    }

    /// Projects a plane-local point to `(screen position, depth)`; larger depth is nearer.
    pub fn project(&self, local: Vec3, rect: Rect) -> (Pos2, f32) {
        let r = self.rotate(local);
        let c = rect.center() + self.pan;
        (Pos2::new(c.x + r.x * self.zoom, c.y - r.y * self.zoom), r.z)
    }

    /// Restores the default face-on view of the tangent plane.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Orients the camera so `axis` points straight at the viewer.
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
    }

    /// Applies drag/scroll/pinch input from the viewport `response`.
    /// `primary_pans` makes left-drag pan instead of orbit (used for the flat 2D map).
    pub fn handle_input(&mut self, ui: &Ui, response: &Response, primary_pans: bool) {
        let delta = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);

        // Left drag orbits (or pans); right/middle drag always pans.
        if response.dragged_by(PointerButton::Primary) {
            if shift || primary_pans {
                self.pan += delta;
            } else {
                self.yaw += delta.x * ORBIT_RAD_PER_PX;
                self.pitch = (self.pitch + delta.y * ORBIT_RAD_PER_PX).clamp(-FRAC_PI_2, FRAC_PI_2);
            }
        } else if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
            self.pan += delta;
        }

        // Zoom only when the pointer is over the viewport, so side-panel scrolling is unaffected.
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = pinch * (scroll * ZOOM_PER_SCROLL).exp();
            self.zoom = (self.zoom * factor).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        }
    }
}
