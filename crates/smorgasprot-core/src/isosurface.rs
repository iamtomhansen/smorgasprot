//! Dual Surface Nets isosurface extraction and closed-mesh geometric integrals.
//!
//! ## Summary
//! Provides [`extract_dual_surface_net`] to extract the `psi < 0` zero-isosurface from a regular
//! 3D scalar field using Dual Surface Nets with constrained Laplacian smoothing, and
//! [`mesh_volume_area_centroid`] / [`compute_mesh_volume_a3`] for divergence-theorem volume,
//! surface area, and centroid calculation.
//!
//! ## Considerations
//! - **Constrained smoothing**: Dual vertices are smoothed toward neighbor centroids and clamped
//!   to their generating voxel cell (`[cell_min, cell_min + step]`) to preserve sharp features
//!   without self-intersection.
//! - **Normals**: Vertex normals are estimated via central differences on the `psi` grid.
//!
//! ## References
//! - Gibson, S. F. F. (1998). Constrained elastic surface nets: generating smooth surfaces from
//!   binary segmented data. https://doi.org/10.1007/BFb0056277
//!
//! ## TODO
//! - Consider trilinear interpolation in normal estimation for smoother shading on coarse grids.

use crate::model::{GlobalAtomIdx, SurfaceMesh};
use glam::Vec3;

/// Emits two counter-clockwise triangles for a quad connecting 4 dual vertices around a
/// sign-changing grid edge.
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

/// Extracts the `psi < 0` zero-isosurface via Dual Surface Nets with constrained Laplacian
/// smoothing (Gibson, 1998).
#[allow(clippy::too_many_arguments)]
pub fn extract_dual_surface_net(
    psi: &[f32],
    nx: usize,
    ny: usize,
    nz: usize,
    min_pt: Vec3,
    step: f32,
    nearest_atom_grid: &[GlobalAtomIdx],
    best_dist_grid: &[f32],
    fallback_atom: GlobalAtomIdx,
) -> SurfaceMesh {
    let total_pts = nx * ny * nz;
    let stride_z = 1usize;
    let stride_y = nz;
    let stride_x = ny * nz;
    let inv_step = 1.0 / step;

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

    // 1. Place one dual vertex per sign-changing voxel cell at the mean edge-crossing point.
    let mut cell_to_vert = vec![u32::MAX; total_pts];
    let mut dual_positions: Vec<Vec3> = Vec::new();
    let mut dual_cell_min: Vec<Vec3> = Vec::new();
    let mut dual_nearest: Vec<GlobalAtomIdx> = Vec::new();

    for ix in 0..(nx - 1) {
        for iy in 0..(ny - 1) {
            for iz in 0..(nz - 1) {
                let mut corner_vals = [0.0_f32; 8];
                let mut mask = 0u8;
                let mut cell_atom = fallback_atom;
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

    // 2. Emit dual quads (2 triangles each) across every sign-changing X, Y, and Z grid edge.
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

    // 3. Constrained Laplacian smoothing (2 passes, clamped to generating cell).
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

    // 4. Compute surface normals via central difference on the psi grid.
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

/// Computes `(volume_a3, surface_area_a2, centroid)` for a closed triangle mesh using the
/// divergence theorem (`V = (1/6) |∑ v0 · (v1 × v2)|`).
pub fn mesh_volume_area_centroid(mesh: &SurfaceMesh) -> (f32, f32, [f32; 3]) {
    if mesh.vertices.is_empty() || mesh.triangles.is_empty() {
        return (0.0, 0.0, [0.0; 3]);
    }
    let mut signed_vol_6 = 0.0_f64;
    let mut area = 0.0_f64;
    for &[i0, i1, i2] in &mesh.triangles {
        let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
        if u0 < mesh.vertices.len() && u1 < mesh.vertices.len() && u2 < mesh.vertices.len() {
            let v0 = Vec3::from(mesh.vertices[u0]);
            let v1 = Vec3::from(mesh.vertices[u1]);
            let v2 = Vec3::from(mesh.vertices[u2]);
            signed_vol_6 += v0.dot(v1.cross(v2)) as f64;
            area += 0.5 * (v1 - v0).cross(v2 - v0).length() as f64;
        }
    }
    let mut sum = Vec3::ZERO;
    for &v in &mesh.vertices {
        sum += Vec3::from(v);
    }
    let centroid = (sum / mesh.vertices.len() as f32).to_array();
    ((signed_vol_6 / 6.0).abs() as f32, area as f32, centroid)
}

/// Computes the enclosed physical volume of a triangle mesh in cubic Angstroms (Å³) using the
/// divergence theorem.
pub fn compute_mesh_volume_a3(mesh: &SurfaceMesh) -> f32 {
    mesh_volume_area_centroid(mesh).0
}
