//! World space → interface plane-local space `(u, v, n)`, plus the flatten/butterfly morph.
//! Everything the camera projects is first expressed in this frame.

use crate::state::ExplodeParams;
use glam::Vec3;
use smorgasprot_core::{InterfaceSide, ProjectionPlane};

/// Small ±n offset keeping the two flat sheets from z-fighting (Å).
const SHEET_OFFSET_A: f32 = 0.08;

/// Orthonormal interface frame built from a [`ProjectionPlane`].
#[derive(Copy, Clone, Debug)]
pub struct PlaneFrame {
    origin: Vec3,
    u: Vec3,
    v: Vec3,
    n: Vec3,
    /// `u` coordinate of the open-book hinge line.
    pub hinge_u: f32,
}

impl From<&ProjectionPlane> for PlaneFrame {
    fn from(p: &ProjectionPlane) -> Self {
        Self {
            origin: p.origin.into(),
            u: p.axis_u.into(),
            v: p.axis_v.into(),
            n: p.normal.into(),
            hinge_u: p.hinge_u,
        }
    }
}

impl PlaneFrame {
    /// Expresses a world-space point in plane-local `(u, v, n)` coordinates.
    pub fn to_local(&self, world: [f32; 3]) -> Vec3 {
        let rel = Vec3::from(world) - self.origin;
        Vec3::new(rel.dot(self.u), rel.dot(self.v), rel.dot(self.n))
    }

    /// Interpolates a point from its true 3D position (`flatten_t = 0`) to its flat map position (`1`).
    /// Side B's flat position is hinged open by `book_t × 180°` around `u = hinge_u`.
    pub fn morph(&self, world: [f32; 3], uv: [f32; 2], side: InterfaceSide, flatten_t: f32, book_t: f32) -> Vec3 {
        let p3 = self.to_local(world);
        let flat = if side == InterfaceSide::SideA {
            Vec3::new(uv[0], uv[1], SHEET_OFFSET_A)
        } else {
            // Rotate about the hinge line (parallel to v) in the u–n plane.
            let (s, c) = (book_t * std::f32::consts::PI).sin_cos();
            let du = uv[0] - self.hinge_u;
            Vec3::new(self.hinge_u + du * c, uv[1], -du * s - SHEET_OFFSET_A * c)
        };
        p3.lerp(flat, flatten_t)
    }
}

/// Translation along ±n applied to one side when the complex is exploded.
pub fn explode_offset(side: InterfaceSide, explode_t: f32, params: &ExplodeParams) -> Vec3 {
    let d = 0.5 * explode_t * params.max_explode_a;
    match side {
        InterfaceSide::SideA => Vec3::new(0.0, 0.0, d),
        InterfaceSide::SideB => Vec3::new(0.0, 0.0, -d),
        InterfaceSide::None => Vec3::ZERO,
    }
}
