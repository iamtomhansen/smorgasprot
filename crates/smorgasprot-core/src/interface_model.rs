//! 3D geometric modeling and manifold representations of protein-protein interfaces.
//!
//! ## Works referenced
//! - **Connolly, M. L. (1986).** "Shape complementarity in protein-protein interfaces."
//!   *Biopolymers*, 25(7), 1229–1247. (Differential geometry of macromolecular contact surfaces
//!   and inter-chain boundary manifolds).
//! - **Halperin, I., Ma, B., Wolfson, H., & Nussinov, R. (2002).** "Principles of docking: An
//!   overview of search algorithms and a guide to scoring functions." *Proteins*, 47(4), 409–443.
//!   (Interface separating surfaces, cavity representations, and contact envelopes).

use crate::model::{AtomNode, GlobalAtomIdx, InterfaceEdge, ProjectionPlane, SurfaceMesh};
use glam::{Vec2, Vec3};

/// Bounding-box UV margin (in Angstroms) added around cross-interface contact midpoints
/// when constructing the interface separator sheet mesh.
pub const SHEET_UV_PADDING_A: f32 = 2.0;

/// Grid resolution (N x N) of the interface separator sheet mesh.
pub const SHEET_GRID_RES: usize = 18;

/// Standard deviation sigma (in Angstroms) of the Gaussian kernel used to interpolate the 3D normal
/// height h(u, v) of the inter-chain separator sheet across contact midpoints.
pub const SHEET_GAUSSIAN_SIGMA_A: f32 = 2.5;

/// Maximum UV distance (in Angstroms) from an active cross-interface contact midpoint within which
/// sheet mesh vertices are emitted. Leaves true physical holes in the sheet where the interface has
/// an internal cavity.
pub const SHEET_FOOTPRINT_RADIUS_A: f32 = 3.2;

/// Builds an approximated 3D inter-chain interface surface sheet mesh separating `SideA` and `SideB`.
///
/// In the `Complex` tab, this mesh floats directly between the interacting chains when
/// `Display Interface` is set to `Smoothed SDF Sheet`.
pub fn build_interface_sheet_mesh(
    plane: &ProjectionPlane,
    nodes: &[AtomNode],
    edges: &[InterfaceEdge],
) -> SurfaceMesh {
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);
    let normal = Vec3::from(plane.normal);

    // Collect 3D midpoints of all cross-interface contacts projected into (u, v, h)
    let mut midpoints_uvh: Vec<(Vec2, f32, GlobalAtomIdx)> = Vec::new();
    for edge in edges {
        if !edge.is_cross_interface {
            continue;
        }
        let na = &nodes[edge.a.0 as usize];
        let nb = &nodes[edge.b.0 as usize];
        let mid_3d = 0.5 * (Vec3::from(na.pos_3d) + Vec3::from(nb.pos_3d));
        let rel = mid_3d - origin;
        let uv = Vec2::new(rel.dot(axis_u), rel.dot(axis_v));
        let h = rel.dot(normal);
        midpoints_uvh.push((uv, h, na.global_atom));
    }

    if midpoints_uvh.is_empty() {
        return SurfaceMesh::default();
    }

    let mut min_uv = Vec2::splat(f32::INFINITY);
    let mut max_uv = Vec2::splat(f32::NEG_INFINITY);
    for &(uv, _, _) in &midpoints_uvh {
        min_uv = min_uv.min(uv);
        max_uv = max_uv.max(uv);
    }
    min_uv -= Vec2::splat(SHEET_UV_PADDING_A);
    max_uv += Vec2::splat(SHEET_UV_PADDING_A);

    let grid_n = SHEET_GRID_RES;
    let step_u = (max_uv.x - min_uv.x).max(1.0) / ((grid_n - 1) as f32);
    let step_v = (max_uv.y - min_uv.y).max(1.0) / ((grid_n - 1) as f32);
    let sigma_sq = SHEET_GAUSSIAN_SIGMA_A * SHEET_GAUSSIAN_SIGMA_A;
    let footprint_r2 = SHEET_FOOTPRINT_RADIUS_A * SHEET_FOOTPRINT_RADIUS_A;

    let mut mesh = SurfaceMesh::default();
    let mut grid_to_vert = vec![u32::MAX; grid_n * grid_n];

    for iu in 0..grid_n {
        for iv in 0..grid_n {
            let uv = Vec2::new(
                min_uv.x + (iu as f32) * step_u,
                min_uv.y + (iv as f32) * step_v,
            );

            let mut w_sum = 0.0_f32;
            let mut h_sum = 0.0_f32;
            let mut best_d2 = f32::INFINITY;
            let mut nearest_atom = midpoints_uvh[0].2;

            for &(m_uv, m_h, g_atom) in &midpoints_uvh {
                let d2 = uv.distance_squared(m_uv);
                if d2 < best_d2 {
                    best_d2 = d2;
                    nearest_atom = g_atom;
                }
                let w = (-d2 / (2.0 * sigma_sq)).exp();
                w_sum += w;
                h_sum += w * m_h;
            }

            // Only emit vertices within the active contact footprint
            if best_d2 <= footprint_r2 && w_sum > 1e-4 {
                let h = h_sum / w_sum;
                let pos_3d = origin + uv.x * axis_u + uv.y * axis_v + h * normal;
                let v_idx = mesh.vertices.len() as u32;
                mesh.vertices.push(pos_3d.to_array());
                mesh.normals.push(normal.to_array());
                mesh.nearest_atom.push(nearest_atom);
                grid_to_vert[iu * grid_n + iv] = v_idx;
            }
        }
    }

    for iu in 0..(grid_n - 1) {
        for iv in 0..(grid_n - 1) {
            let v00 = grid_to_vert[iu * grid_n + iv];
            let v10 = grid_to_vert[(iu + 1) * grid_n + iv];
            let v01 = grid_to_vert[iu * grid_n + (iv + 1)];
            let v11 = grid_to_vert[(iu + 1) * grid_n + (iv + 1)];

            if v00 != u32::MAX && v10 != u32::MAX && v11 != u32::MAX {
                mesh.triangles.push([v00, v10, v11]);
            }
            if v00 != u32::MAX && v11 != u32::MAX && v01 != u32::MAX {
                mesh.triangles.push([v00, v11, v01]);
            }
        }
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AtomNodeIdx, EdgeKind, InterfaceSide, TerritoryIdx};

    #[test]
    fn test_empty_edges_build_empty_sheet() {
        let plane = ProjectionPlane {
            origin: [0.0, 0.0, 0.0],
            axis_u: [1.0, 0.0, 0.0],
            axis_v: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            hinge_u: -10.0,
        };
        let mesh = build_interface_sheet_mesh(&plane, &[], &[]);
        assert!(mesh.vertices.is_empty());
        assert!(mesh.triangles.is_empty());
    }

    #[test]
    fn test_contact_pair_produces_interface_sheet() {
        let plane = ProjectionPlane {
            origin: [0.0, 0.0, 0.0],
            axis_u: [1.0, 0.0, 0.0],
            axis_v: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            hinge_u: -10.0,
        };
        let nodes = vec![
            AtomNode {
                idx: AtomNodeIdx(0),
                global_atom: GlobalAtomIdx(0),
                territory: TerritoryIdx(0),
                role: InterfaceSide::SideA,
                b_factor: 25.0,
                free_sasa_a2: 10.0,
                complex_sasa_a2: 0.0,
                buried_sasa_a2: 10.0,
                aliphatic_score: 1.0,
                pos_3d: [0.0, 0.0, 1.0],
                pos_uv_raw: [0.0, 0.0],
                pos_uv: [0.0, 0.0],
                is_unsatisfied_buried_polar: false,
                interactions: Vec::new(),
            },
            AtomNode {
                idx: AtomNodeIdx(1),
                global_atom: GlobalAtomIdx(1),
                territory: TerritoryIdx(1),
                role: InterfaceSide::SideB,
                b_factor: 30.0,
                free_sasa_a2: 10.0,
                complex_sasa_a2: 0.0,
                buried_sasa_a2: 10.0,
                aliphatic_score: 1.0,
                pos_3d: [0.0, 0.0, -1.0],
                pos_uv_raw: [0.0, 0.0],
                pos_uv: [0.0, 0.0],
                is_unsatisfied_buried_polar: false,
                interactions: Vec::new(),
            },
        ];
        let edges = vec![InterfaceEdge {
            a: AtomNodeIdx(0),
            b: AtomNodeIdx(1),
            is_cross_interface: true,
            kind: EdgeKind::VanDerWaalsContact,
            dist_3d_a: 2.0,
            dist_2d_a: 2.0,
            strain_2d_a: 0.0,
            overlap_a: 0.0,
        }];

        let mesh = build_interface_sheet_mesh(&plane, &nodes, &edges);
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.triangles.is_empty());
        // Normal of the sheet should point along plane normal [0, 0, 1]
        assert_eq!(mesh.normals[0], [0.0, 0.0, 1.0]);
    }
}
