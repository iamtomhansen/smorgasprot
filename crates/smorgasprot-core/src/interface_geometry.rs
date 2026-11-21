//! 3D geometric modeling and manifold representations of protein-protein interfaces.
//!
//! ## Works referenced
//! - **Connolly, M. L. (1986).** "Shape complementarity in protein-protein interfaces."
//!   *Biopolymers*, 25(7), 1229–1247. (Differential geometry of macromolecular contact surfaces
//!   and inter-chain boundary manifolds).
//! - **Halperin, I., Ma, B., Wolfson, H., & Nussinov, R. (2002).** "Principles of docking: An
//!   overview of search algorithms and a guide to scoring functions." *Proteins*, 47(4), 409–443.
//!   (Interface separating surfaces, cavity representations, and contact envelopes).

use crate::model::{
    AtomNode, ComplexStructure, GlobalAtomIdx, InterfaceEdge, InterfaceGeometry, InterfaceMap,
    InterfaceSide, ProjectionPlane, SurfaceMesh,
};
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

/// Maximum surface-to-surface gap width (in Angstroms) between Side A and Side B heavy atoms
/// considered part of the interface negative space / sub-solvent void.
///
/// A threshold of 2.8 A corresponds to the diameter of a spherical water probe (1.4 A radius).
/// Any gap narrower than 2.8 A cannot permit free passage of bulk water and represents a
/// trapped interfacial void or packing defect.
pub const INTERFACE_VOID_MAX_GAP_A: f32 = 2.8;

/// Polynomial smooth-minimum blending parameter (in Angstroms) for combining overlapping atomic
/// spheres into the per-side signed-distance field.
pub const INTERFACE_VOID_SMOOTH_K_A: f32 = 0.55;

/// Voxel grid step (in Angstroms) for the interface negative space 3D isosurface grid.
/// 0.95 A gives fine, sub-Angstrom resolution of narrow interfacial crevices and pockets.
pub const INTERFACE_VOID_GRID_STEP_A: f32 = 0.95;

/// Margin (in Angstroms) added around the 3D bounding box of interfacial contact atoms.
pub const INTERFACE_VOID_BOUNDING_MARGIN_A: f32 = 3.5;

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

/// Extracts the 3D two-sided interface contact envelope mesh by duplicating in-place the contact
/// surface triangles from `SideA` and `SideB` chain surface meshes that touch interfacial atoms.
///
/// This provides a standalone interface mesh object representing the true two-sided molecular skin
/// boundary between interacting partners, enabling independent opacity, coloring, and analysis.
pub fn extract_interface_envelope_mesh(
    chain_meshes: &[SurfaceMesh],
    complex: &ComplexStructure,
    nodes: &[AtomNode],
) -> SurfaceMesh {
    if chain_meshes.is_empty() || nodes.is_empty() {
        return SurfaceMesh::default();
    }

    let mut is_interfacial = vec![false; complex.atoms.len()];
    for node in nodes {
        let a_idx = node.global_atom.0 as usize;
        if a_idx < is_interfacial.len() {
            is_interfacial[a_idx] = true;
        }
    }

    let mut envelope = SurfaceMesh::default();

    for (c_idx, chain_mesh) in chain_meshes.iter().enumerate() {
        if c_idx >= complex.chains.len() {
            continue;
        }
        let role = complex.chain_side(&complex.chains[c_idx].chain_id);
        if !role.is_assigned() {
            continue;
        }

        let mut local_remap = vec![u32::MAX; chain_mesh.vertices.len()];

        for &[i0, i1, i2] in &chain_mesh.triangles {
            let u0 = i0 as usize;
            let u1 = i1 as usize;
            let u2 = i2 as usize;
            if u0 >= chain_mesh.vertices.len()
                || u1 >= chain_mesh.vertices.len()
                || u2 >= chain_mesh.vertices.len()
            {
                continue;
            }

            let a0 = chain_mesh.nearest_atom[u0].0 as usize;
            let a1 = chain_mesh.nearest_atom[u1].0 as usize;
            let a2 = chain_mesh.nearest_atom[u2].0 as usize;

            let touch_count = (is_interfacial.get(a0).copied().unwrap_or(false) as usize)
                + (is_interfacial.get(a1).copied().unwrap_or(false) as usize)
                + (is_interfacial.get(a2).copied().unwrap_or(false) as usize);

            // Include triangle if at least 1 vertex touches an interfacial atom
            if touch_count >= 1 {
                let get_or_insert =
                    |v_idx: usize, remap: &mut [u32], mesh: &SurfaceMesh, out: &mut SurfaceMesh| -> u32 {
                        if remap[v_idx] != u32::MAX {
                            return remap[v_idx];
                        }
                        let new_idx = out.vertices.len() as u32;
                        out.vertices.push(mesh.vertices[v_idx]);
                        out.normals.push(mesh.normals[v_idx]);
                        out.nearest_atom.push(mesh.nearest_atom[v_idx]);
                        remap[v_idx] = new_idx;
                        new_idx
                    };

                let v0 = get_or_insert(u0, &mut local_remap, chain_mesh, &mut envelope);
                let v1 = get_or_insert(u1, &mut local_remap, chain_mesh, &mut envelope);
                let v2 = get_or_insert(u2, &mut local_remap, chain_mesh, &mut envelope);
                envelope.triangles.push([v0, v1, v2]);
            }
        }
    }

    envelope
}

/// Quadratic polynomial smooth-minimum of two signed distances `a` and `b` with blending radius `k`.
#[inline]
fn smin_poly(a: f32, b: f32, k: f32) -> f32 {
    if k <= 1e-5 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// Emits two counter-clockwise triangles for a quad connecting 4 dual vertices around a sign-changing grid edge.
#[inline]
fn emit_quad(triangles: &mut Vec<[u32; 3]>, v0: u32, v1: u32, v2: u32, v3: u32, flip: bool) {
    if v0 == u32::MAX || v1 == u32::MAX || v2 == u32::MAX || v3 == u32::MAX {
        return;
    }
    if flip {
        triangles.push([v0, v2, v1]);
        triangles.push([v0, v3, v2]);
    } else {
        triangles.push([v0, v1, v2]);
        triangles.push([v0, v2, v3]);
    }
}

/// Builds a 3D negative-space triangle mesh representing the sub-solvent voids and packing crevices
/// trapped between `SideA` and `SideB` at the interface.
///
/// Evaluates the dual-SDF field:
///   psi(p) = max( -min(phi_A(p), phi_B(p)),  max(phi_A(p), phi_B(p)) - gap_cutoff )
/// where psi(p) < 0 defines empty space that lies outside both atomic skins (min > 0)
/// yet within the narrow inter-chain contact corridor (max < gap_cutoff).
pub fn build_interface_negative_mesh(
    complex: &ComplexStructure,
    nodes: &[AtomNode],
) -> SurfaceMesh {
    if nodes.is_empty() {
        return SurfaceMesh::default();
    }

    let mut side_a_atoms: Vec<(Vec3, f32, GlobalAtomIdx)> = Vec::new();
    let mut side_b_atoms: Vec<(Vec3, f32, GlobalAtomIdx)> = Vec::new();
    let mut min_pt = Vec3::splat(f32::INFINITY);
    let mut max_pt = Vec3::splat(f32::NEG_INFINITY);

    for node in nodes {
        let a_idx = node.global_atom.0 as usize;
        if a_idx >= complex.atoms.len() {
            continue;
        }
        let atom = &complex.atoms[a_idx];
        let pos = Vec3::from(atom.pos_3d);
        let r = atom.vdw_radius_a + 0.15;
        min_pt = min_pt.min(pos);
        max_pt = max_pt.max(pos);

        match node.role {
            InterfaceSide::SideA => side_a_atoms.push((pos, r, node.global_atom)),
            InterfaceSide::SideB => side_b_atoms.push((pos, r, node.global_atom)),
            InterfaceSide::None => {}
        }
    }

    if side_a_atoms.is_empty() || side_b_atoms.is_empty() {
        return SurfaceMesh::default();
    }

    min_pt -= Vec3::splat(INTERFACE_VOID_BOUNDING_MARGIN_A);
    max_pt += Vec3::splat(INTERFACE_VOID_BOUNDING_MARGIN_A);

    let step = INTERFACE_VOID_GRID_STEP_A;
    let inv_step = 1.0 / step;
    let nx = (((max_pt.x - min_pt.x) * inv_step).ceil() as usize + 1).max(4);
    let ny = (((max_pt.y - min_pt.y) * inv_step).ceil() as usize + 1).max(4);
    let nz = (((max_pt.z - min_pt.z) * inv_step).ceil() as usize + 1).max(4);
    let stride_z = 1usize;
    let stride_y = nz;
    let stride_x = ny * nz;
    let total_pts = nx * ny * nz;

    let mut phi_a = vec![10.0_f32; total_pts];
    let mut phi_b = vec![10.0_f32; total_pts];
    let mut nearest_atom_grid = vec![nodes[0].global_atom; total_pts];
    let mut best_dist_grid = vec![f32::INFINITY; total_pts];

    let mut splat_atoms = |atoms: &[(Vec3, f32, GlobalAtomIdx)], phi: &mut [f32]| {
        for &(center, r_i, g_atom) in atoms {
            let reach = r_i + INTERFACE_VOID_MAX_GAP_A + step * 1.5;
            let ix_min = (((center.x - reach - min_pt.x) * inv_step).floor() as isize).max(0) as usize;
            let ix_max = (((center.x + reach - min_pt.x) * inv_step).ceil() as usize).min(nx.saturating_sub(1));
            let iy_min = (((center.y - reach - min_pt.y) * inv_step).floor() as isize).max(0) as usize;
            let iy_max = (((center.y + reach - min_pt.y) * inv_step).ceil() as usize).min(ny.saturating_sub(1));
            let iz_min = (((center.z - reach - min_pt.z) * inv_step).floor() as isize).max(0) as usize;
            let iz_max = (((center.z + reach - min_pt.z) * inv_step).ceil() as usize).min(nz.saturating_sub(1));

            for ix in ix_min..=ix_max {
                let gx = min_pt.x + (ix as f32) * step;
                let dx2 = (gx - center.x) * (gx - center.x);
                for iy in iy_min..=iy_max {
                    let gy = min_pt.y + (iy as f32) * step;
                    let dxy2 = dx2 + (gy - center.y) * (gy - center.y);
                    let base_idx = ix * stride_x + iy * stride_y;
                    for iz in iz_min..=iz_max {
                        let gz = min_pt.z + (iz as f32) * step;
                        let d2 = dxy2 + (gz - center.z) * (gz - center.z);
                        if d2 > reach * reach {
                            continue;
                        }
                        let d_surf = d2.sqrt() - r_i;
                        let g_idx = base_idx + iz;
                        phi[g_idx] = smin_poly(phi[g_idx], d_surf, INTERFACE_VOID_SMOOTH_K_A);
                        let abs_d = d_surf.abs();
                        if abs_d < best_dist_grid[g_idx] {
                            best_dist_grid[g_idx] = abs_d;
                            nearest_atom_grid[g_idx] = g_atom;
                        }
                    }
                }
            }
        }
    };

    splat_atoms(&side_a_atoms, &mut phi_a);
    splat_atoms(&side_b_atoms, &mut phi_b);

    // Compute psi(p) = max( -min(phi_a, phi_b), max(phi_a, phi_b) - gap_cutoff )
    let mut psi = vec![10.0_f32; total_pts];
    for g in 0..total_pts {
        let da = phi_a[g];
        let db = phi_b[g];
        let d_min = da.min(db);
        let d_max = da.max(db);
        psi[g] = (-d_min).max(d_max - INTERFACE_VOID_MAX_GAP_A);
    }

    // Extract zero-isosurface via Dual Surface Nets
    let cube_corners: [[usize; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
    ];
    let cube_edges: [[usize; 2]; 12] = [
        [0, 1],
        [1, 2],
        [2, 3],
        [3, 0],
        [4, 5],
        [5, 6],
        [6, 7],
        [7, 4],
        [0, 4],
        [1, 5],
        [2, 6],
        [3, 7],
    ];

    let mut cell_to_vert = vec![u32::MAX; total_pts];
    let mut dual_positions: Vec<Vec3> = Vec::new();
    let mut dual_cell_min: Vec<Vec3> = Vec::new();
    let mut dual_nearest: Vec<GlobalAtomIdx> = Vec::new();

    for ix in 0..(nx - 1) {
        for iy in 0..(ny - 1) {
            for iz in 0..(nz - 1) {
                let mut corner_vals = [0.0_f32; 8];
                let mut mask = 0u8;
                let mut cell_atom = nodes[0].global_atom;
                let mut best_c_dist = f32::INFINITY;

                for (c_idx, &[cx, cy, cz]) in cube_corners.iter().enumerate() {
                    let g_idx = (ix + cx) * stride_x + (iy + cy) * stride_y + (iz + cz);
                    let val = psi[g_idx];
                    corner_vals[c_idx] = val;
                    if val < 0.0 {
                        mask |= 1 << c_idx;
                    }
                    if best_dist_grid[g_idx] < best_c_dist {
                        best_c_dist = best_dist_grid[g_idx];
                        cell_atom = nearest_atom_grid[g_idx];
                    }
                }

                if mask == 0 || mask == 0xFF {
                    continue;
                }

                let base_pos = min_pt + Vec3::new(ix as f32, iy as f32, iz as f32) * step;
                let mut cross_sum = Vec3::ZERO;
                let mut cross_count = 0.0_f32;

                for &[c0, c1] in &cube_edges {
                    let v0 = corner_vals[c0];
                    let v1 = corner_vals[c1];
                    if (v0 < 0.0) != (v1 < 0.0) {
                        let denom = v0 - v1;
                        let t = if denom.abs() > 1e-6 {
                            (v0 / denom).clamp(0.0, 1.0)
                        } else {
                            0.5
                        };
                        let p0 = Vec3::new(
                            cube_corners[c0][0] as f32,
                            cube_corners[c0][1] as f32,
                            cube_corners[c0][2] as f32,
                        );
                        let p1 = Vec3::new(
                            cube_corners[c1][0] as f32,
                            cube_corners[c1][1] as f32,
                            cube_corners[c1][2] as f32,
                        );
                        cross_sum += base_pos + p0.lerp(p1, t) * step;
                        cross_count += 1.0;
                    }
                }

                if cross_count > 0.0 {
                    let v_idx = dual_positions.len() as u32;
                    dual_positions.push(cross_sum / cross_count);
                    dual_cell_min.push(base_pos);
                    dual_nearest.push(cell_atom);
                    cell_to_vert[ix * stride_x + iy * stride_y + iz] = v_idx;
                }
            }
        }
    }

    if dual_positions.is_empty() {
        return SurfaceMesh::default();
    }

    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(dual_positions.len() * 2);

    for ix in 0..(nx - 1) {
        for iy in 1..(ny - 1) {
            for iz in 1..(nz - 1) {
                let g0 = ix * stride_x + iy * stride_y + iz;
                let s0 = psi[g0] < 0.0;
                let sx = psi[g0 + stride_x] < 0.0;
                if s0 != sx {
                    let c0 = cell_to_vert[ix * stride_x + (iy - 1) * stride_y + (iz - 1)];
                    let c1 = cell_to_vert[ix * stride_x + iy * stride_y + (iz - 1)];
                    let c2 = cell_to_vert[ix * stride_x + iy * stride_y + iz];
                    let c3 = cell_to_vert[ix * stride_x + (iy - 1) * stride_y + iz];
                    emit_quad(&mut triangles, c0, c1, c2, c3, !s0);
                }
            }
        }
    }

    for ix in 1..(nx - 1) {
        for iy in 0..(ny - 1) {
            for iz in 1..(nz - 1) {
                let g0 = ix * stride_x + iy * stride_y + iz;
                let s0 = psi[g0] < 0.0;
                let sy = psi[g0 + stride_y] < 0.0;
                if s0 != sy {
                    let c0 = cell_to_vert[(ix - 1) * stride_x + iy * stride_y + (iz - 1)];
                    let c1 = cell_to_vert[(ix - 1) * stride_x + iy * stride_y + iz];
                    let c2 = cell_to_vert[ix * stride_x + iy * stride_y + iz];
                    let c3 = cell_to_vert[ix * stride_x + iy * stride_y + (iz - 1)];
                    emit_quad(&mut triangles, c0, c1, c2, c3, !s0);
                }
            }
        }
    }

    for ix in 1..(nx - 1) {
        for iy in 1..(ny - 1) {
            for iz in 0..(nz - 1) {
                let g0 = ix * stride_x + iy * stride_y + iz;
                let s0 = psi[g0] < 0.0;
                let sz = psi[g0 + stride_z] < 0.0;
                if s0 != sz {
                    let c0 = cell_to_vert[(ix - 1) * stride_x + (iy - 1) * stride_y + iz];
                    let c1 = cell_to_vert[ix * stride_x + (iy - 1) * stride_y + iz];
                    let c2 = cell_to_vert[ix * stride_x + iy * stride_y + iz];
                    let c3 = cell_to_vert[(ix - 1) * stride_x + iy * stride_y + iz];
                    emit_quad(&mut triangles, c0, c1, c2, c3, !s0);
                }
            }
        }
    }

    // Constrained Laplacian smoothing (2 passes, clamped to generating cell)
    for _ in 0..2 {
        let mut accum = vec![Vec3::ZERO; dual_positions.len()];
        let mut counts = vec![0.0_f32; dual_positions.len()];
        for &[i0, i1, i2] in &triangles {
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
            let (p0, p1, p2) = (dual_positions[u0], dual_positions[u1], dual_positions[u2]);
            accum[u0] += p1 + p2;
            counts[u0] += 2.0;
            accum[u1] += p0 + p2;
            counts[u1] += 2.0;
            accum[u2] += p0 + p1;
            counts[u2] += 2.0;
        }
        for i in 0..dual_positions.len() {
            if counts[i] > 0.0 {
                let avg = accum[i] / counts[i];
                let cell_min = dual_cell_min[i];
                let cell_max = cell_min + Vec3::splat(step);
                dual_positions[i] = dual_positions[i].lerp(avg, 0.40).clamp(cell_min, cell_max);
            }
        }
    }

    // Compute surface normals via central difference of psi grid
    let sample_psi = |pos: Vec3| -> f32 {
        let gx = ((pos.x - min_pt.x) * inv_step).round() as isize;
        let gy = ((pos.y - min_pt.y) * inv_step).round() as isize;
        let gz = ((pos.z - min_pt.z) * inv_step).round() as isize;
        let cx = (gx.max(0) as usize).min(nx - 1);
        let cy = (gy.max(0) as usize).min(ny - 1);
        let cz = (gz.max(0) as usize).min(nz - 1);
        psi[cx * stride_x + cy * stride_y + cz]
    };

    let eps = step * 0.5;
    let mut normals = Vec::with_capacity(dual_positions.len());
    let mut vertices = Vec::with_capacity(dual_positions.len());

    for pos in &dual_positions {
        let p = *pos;
        let dx = sample_psi(p + Vec3::X * eps) - sample_psi(p - Vec3::X * eps);
        let dy = sample_psi(p + Vec3::Y * eps) - sample_psi(p - Vec3::Y * eps);
        let dz = sample_psi(p + Vec3::Z * eps) - sample_psi(p - Vec3::Z * eps);
        let g = Vec3::new(dx, dy, dz);
        let n = if g.length_squared() > 1e-6 {
            g.normalize()
        } else {
            Vec3::Y
        };
        vertices.push(p.to_array());
        normals.push(n.to_array());
    }

    SurfaceMesh {
        vertices,
        normals,
        nearest_atom: dual_nearest,
        triangles,
    }
}

/// Computes the enclosed physical volume of a triangle mesh in cubic Angstroms (Å³)
/// using the divergence theorem:
///
/// V = (1 / 6) * |∑_{(v0, v1, v2) ∈ triangles} v0 · (v1 × v2)|
pub fn compute_mesh_volume_a3(mesh: &SurfaceMesh) -> f32 {
    let mut signed_vol_6 = 0.0_f64;
    for &[i0, i1, i2] in &mesh.triangles {
        let u0 = i0 as usize;
        let u1 = i1 as usize;
        let u2 = i2 as usize;
        if u0 < mesh.vertices.len() && u1 < mesh.vertices.len() && u2 < mesh.vertices.len() {
            let v0 = Vec3::from(mesh.vertices[u0]);
            let v1 = Vec3::from(mesh.vertices[u1]);
            let v2 = Vec3::from(mesh.vertices[u2]);
            signed_vol_6 += (v0.dot(v1.cross(v2))) as f64;
        }
    }
    (signed_vol_6 / 6.0).abs() as f32
}

/// Builds the comprehensive 3D interface geometry representations:
/// 1. Smoothed inter-chain interface separator sheet mesh.
/// 2. 2-sided duplicated in-place contact envelope mesh.
/// 3. Negative-space sub-solvent cavity / void mesh.
/// 4. Interfacial void volume in cubic Angstroms (Å³).
pub fn build_interface_geometry(
    complex: &ComplexStructure,
    map: &InterfaceMap,
    chain_meshes: &[SurfaceMesh],
) -> InterfaceGeometry {
    let sheet_mesh = build_interface_sheet_mesh(&map.plane, &map.nodes, &map.edges);
    let envelope_mesh = extract_interface_envelope_mesh(chain_meshes, complex, &map.nodes);
    let negative_mesh = build_interface_negative_mesh(complex, &map.nodes);
    let void_volume_a3 = compute_mesh_volume_a3(&negative_mesh);

    InterfaceGeometry {
        sheet_mesh,
        envelope_mesh,
        negative_mesh,
        void_volume_a3,
    }
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
