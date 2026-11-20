//! 3D molecular surface mesh generation and inter-chain interface separator sheet mesh.

use crate::model::{
    ChainIdx, ComplexStructure, GlobalAtomIdx, InterfaceEdge, AtomNode, SurfaceMesh,
    ProjectionPlane,
};
use crate::spatial::SpatialGrid;
use glam::{Vec2, Vec3};

/// Subdivided octahedron template (32 triangles, 18 vertices on the unit sphere).
fn unit_geodesic_sphere() -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let base_verts = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let base_tris: [[usize; 3]; 8] = [
        [0, 2, 4],
        [4, 2, 1],
        [1, 2, 5],
        [5, 2, 0],
        [4, 3, 0],
        [1, 3, 4],
        [5, 3, 1],
        [0, 3, 5],
    ];

    let mut verts: Vec<Vec3> = base_verts.to_vec();
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(32);

    let mut find_or_insert = |v: Vec3| -> u32 {
        let vn = v.normalize_or_zero();
        for (idx, &existing) in verts.iter().enumerate() {
            if existing.distance_squared(vn) < 1e-4 {
                return idx as u32;
            }
        }
        let idx = verts.len() as u32;
        verts.push(vn);
        idx
    };

    for [i0, i1, i2] in base_tris {
        let v0 = base_verts[i0];
        let v1 = base_verts[i1];
        let v2 = base_verts[i2];
        let m01 = find_or_insert(0.5 * (v0 + v1));
        let m12 = find_or_insert(0.5 * (v1 + v2));
        let m20 = find_or_insert(0.5 * (v2 + v0));
        let c0 = i0 as u32;
        let c1 = i1 as u32;
        let c2 = i2 as u32;

        tris.push([c0, m01, m20]);
        tris.push([m01, c1, m12]);
        tris.push([m20, m12, c2]);
        tris.push([m01, m12, m20]);
    }

    (verts, tris)
}

/// Builds a 3D solvent-accessible surface triangle mesh for a single chain in `complex`.
/// Every surface vertex records its `nearest_atom: GlobalAtomIdx` so that live changes to
/// `SideA` / `SideB` chain assignments immediately highlight interfacial surface vertices in O(1).
pub fn build_chain_surface_mesh(complex: &ComplexStructure, chain_idx: ChainIdx) -> SurfaceMesh {
    let chain = &complex.chains[chain_idx.0 as usize];
    let start_atom = chain.atom_range.0 as usize;
    let end_atom = chain.atom_range.1 as usize;

    let shell_inflate = 0.75_f32;
    let mut grid = SpatialGrid::new(4.5);
    for a_idx in start_atom..end_atom {
        grid.insert(a_idx as u32, complex.atoms[a_idx].pos_3d);
    }

    let (unit_verts, unit_tris) = unit_geodesic_sphere();
    let mut mesh = SurfaceMesh::default();
    let mut nbrs: Vec<(Vec3, f32)> = Vec::with_capacity(48);
    let mut local_to_mesh_idx = vec![u32::MAX; unit_verts.len()];

    for a_idx in start_atom..end_atom {
        let atom = &complex.atoms[a_idx];
        let center = Vec3::from(atom.pos_3d);
        let r_i = atom.vdw_radius_a + shell_inflate;

        nbrs.clear();
        grid.query_radius(atom.pos_3d, r_i + 2.8, |j_u32, pos_j, d2| {
            let j = j_u32 as usize;
            if j == a_idx {
                return;
            }
            let r_j = complex.atoms[j].vdw_radius_a + shell_inflate * 0.92;
            if d2 < (r_i + r_j) * (r_i + r_j) {
                nbrs.push((Vec3::from(pos_j), r_j * r_j));
            }
        });

        local_to_mesh_idx.fill(u32::MAX);

        for (v_local, &u_dir) in unit_verts.iter().enumerate() {
            let v_pos = center + r_i * u_dir;
            let occluded = nbrs
                .iter()
                .any(|&(nbr_pos, r_j_sq)| v_pos.distance_squared(nbr_pos) < r_j_sq);
            if !occluded {
                let m_idx = mesh.vertices.len() as u32;
                mesh.vertices.push(v_pos.to_array());
                mesh.normals.push(u_dir.to_array());
                mesh.nearest_atom.push(GlobalAtomIdx(a_idx as u32));
                local_to_mesh_idx[v_local] = m_idx;
            }
        }

        for &[t0, t1, t2] in &unit_tris {
            let m0 = local_to_mesh_idx[t0 as usize];
            let m1 = local_to_mesh_idx[t1 as usize];
            let m2 = local_to_mesh_idx[t2 as usize];
            if m0 != u32::MAX && m1 != u32::MAX && m2 != u32::MAX {
                mesh.triangles.push([m0, m1, m2]);
            }
        }
    }

    mesh
}

/// Builds an approximated 3D inter-chain interface surface sheet mesh separating `SideA` and `SideB`.
/// In Cartoon mode in the `Complex` tab, this mesh floats directly between the interacting chains.
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
    let pad = 2.0_f32;
    min_uv -= Vec2::splat(pad);
    max_uv += Vec2::splat(pad);

    let grid_n = 18usize;
    let step_u = (max_uv.x - min_uv.x).max(1.0) / ((grid_n - 1) as f32);
    let step_v = (max_uv.y - min_uv.y).max(1.0) / ((grid_n - 1) as f32);
    let sigma_sq = 6.25_f32; // sigma = 2.5 A Gaussian smoothing kernel

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

            // Only emit vertices within the active contact footprint (<= 3.2 A from a contact midpoint)
            if best_d2 <= 3.2 * 3.2 && w_sum > 1e-4 {
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
