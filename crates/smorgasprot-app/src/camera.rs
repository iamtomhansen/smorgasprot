//! Interactive 3D camera, arcball rotation, coordinate transforms, and projection math.

use egui::{Pos2, Rect};
use smorgasprot_core::{InterfaceSide, ProjectionPlane};

/// Interactive 3D camera with yaw/pitch orbit, screen pan, and exponential zoom.
#[derive(Clone, Debug)]
pub struct CameraState {
    pub yaw: f32,
    pub pitch: f32,
    pub pan: [f32; 2],
    pub zoom: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            pan: [0.0, 0.0],
            zoom: 14.5,
        }
    }
}

impl CameraState {
    /// Projects a 3D coordinate in tangent plane local space `[x, y, z]` to screen position `(Pos2, depth)`.
    pub fn project_local_point(&self, local_pt: [f32; 3], rect: Rect) -> (Pos2, f32) {
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let x0 = local_pt[0];
        let y0 = local_pt[1];
        let z0 = local_pt[2];
        let x1 = x0 * cy + z0 * sy;
        let z1 = -x0 * sy + z0 * cy;
        let y2 = y0 * cp - z1 * sp;
        let z2 = y0 * sp + z1 * cp;

        let center = rect.center();
        let sx = center.x + self.pan[0] + x1 * self.zoom;
        let sy = center.y + self.pan[1] - y2 * self.zoom;
        (Pos2::new(sx, sy), z2)
    }

    /// Rotates a 3D vector for the orientation gizmo.
    pub fn rotate_axis_for_gizmo(&self, vec: [f32; 3]) -> [f32; 3] {
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let x0 = vec[0];
        let y0 = vec[1];
        let z0 = vec[2];
        let x1 = x0 * cy + z0 * sy;
        let z1 = -x0 * sy + z0 * cy;
        let y2 = y0 * cp - z1 * sp;
        let z2 = y0 * sp + z1 * cp;
        [x1, y2, z2]
    }
}

/// Transforms a world-space 3D coordinate into tangent plane local coordinates `(u, v, normal)`.
pub fn world_to_plane_local(pos3d: [f32; 3], plane: &ProjectionPlane) -> [f32; 3] {
    let p = glam::Vec3::from(pos3d);
    let orig = glam::Vec3::from(plane.origin);
    let u = glam::Vec3::from(plane.axis_u);
    let v = glam::Vec3::from(plane.axis_v);
    let n = glam::Vec3::from(plane.normal);
    let rel = p - orig;
    [rel.dot(u), rel.dot(v), rel.dot(n)]
}

/// Computes the 3D local coordinate of an atom during continuous 3D -> Bas-Relief -> 2D interpolation
/// and open-book butterfly rotation.
pub fn compute_interface_atom_local_3d(
    pos_3d: [f32; 3],
    pos_uv: [f32; 2],
    role: InterfaceSide,
    plane: &ProjectionPlane,
    flatten_t: f32,
    book_theta: f32,
) -> [f32; 3] {
    let p3_local = glam::Vec3::from(world_to_plane_local(pos_3d, plane));
    let u = pos_uv[0];
    let v = pos_uv[1];
    let sheet_offset = 0.08_f32;
    let p_flat_local = if role == InterfaceSide::SideA {
        glam::Vec3::new(u, v, sheet_offset)
    } else {
        let phi = book_theta * std::f32::consts::PI;
        let du = u - plane.hinge_u;
        let u_rot = plane.hinge_u + du * phi.cos();
        let n_rot = -du * phi.sin() - sheet_offset * phi.cos();
        glam::Vec3::new(u_rot, v, n_rot)
    };
    p3_local.lerp(p_flat_local, flatten_t).to_array()
}

/// Computes the explode translation offset along the tangent plane normal `±n`.
pub fn get_explode_offset_local(role: InterfaceSide, explode_t: f32) -> [f32; 3] {
    let max_explode_a = 24.0_f32;
    let d = 0.5 * explode_t * max_explode_a;
    match role {
        InterfaceSide::SideA => [0.0, 0.0, d],
        InterfaceSide::SideB => [0.0, 0.0, -d],
        InterfaceSide::None => [0.0, 0.0, 0.0],
    }
}
