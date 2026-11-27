//! Visual negative-space mesh of sub-solvent interfacial packing crevices.
//!
//! ## Summary
//! Builds a 3D negative-space triangle mesh ([`build_interface_negative_mesh`]) representing
//! the un-eroded (`r_probe = 0`) smoothed void corridor trapped between `SideA` and `SideB`
//! within `void_max_gap_a`.
//!
//! ## Considerations
//! - **Visualization vs. analysis**: Unlike [`crate::interface::cavities`], this mesh does not
//!   enforce a probe radius, solvent-connectivity partitioning, or local sub-voxel refinement.
//!   Use `cavities::build_interface_cavities` for quantitative cavity/channel metrics.
//! - **Dual-SDF field**: Evaluates
//!   `ψ(p) = max(-min(φ_A(p), φ_B(p)), max(φ_A(p), φ_B(p)) - gap_cutoff)`, where `ψ(p) < 0`
//!   marks empty space outside both smoothed atomic skins yet inside the contact corridor.
//!
//! ## References
//! - Halperin, I., Ma, B., Wolfson, H., Nussinov, R. (2002). Principles of docking: an overview
//!   of search algorithms and a guide to scoring functions. https://doi.org/10.1002/prot.10115
//!
//! ## TODO
//! - Share atom-splatting grid logic with `cavities.rs` if smooth-union blending is generalized.

use super::GeometryParams;
use super::definition::InterfaceSide;
use crate::isosurface::extract_dual_surface_net;
use super::model::AtomNode;
use crate::model::{ComplexStructure, GlobalAtomIdx, SurfaceMesh};
use glam::Vec3;

/// Quadratic polynomial smooth-minimum of two signed distances `a` and `b` with blending radius `k`.
#[inline]
fn smin_poly(a: f32, b: f32, k: f32) -> f32 {
    if k <= 1e-5 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// Builds a 3D negative-space triangle mesh representing the sub-solvent voids and packing
/// crevices trapped between `SideA` and `SideB` at the interface.
pub fn build_interface_negative_mesh(
    complex: &ComplexStructure,
    nodes: &[AtomNode],
    p: &GeometryParams,
) -> SurfaceMesh {
    if nodes.is_empty() {
        return SurfaceMesh::default();
    }

    // 1. Collect SideA and SideB interfacial atoms and compute the padded bounding box.
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
        let r = atom.vdw_radius_a + p.implicit_h_skin_a;
        min_pt = min_pt.min(pos);
        max_pt = max_pt.max(pos);

        match node.side {
            InterfaceSide::SideA => side_a_atoms.push((pos, r, node.global_atom)),
            InterfaceSide::SideB => side_b_atoms.push((pos, r, node.global_atom)),
            InterfaceSide::None => {}
        }
    }

    if side_a_atoms.is_empty() || side_b_atoms.is_empty() {
        return SurfaceMesh::default();
    }

    min_pt -= Vec3::splat(p.void_bounding_margin_a);
    max_pt += Vec3::splat(p.void_bounding_margin_a);

    let step = p.void_grid_step_a;
    let inv_step = 1.0 / step;
    let nx = (((max_pt.x - min_pt.x) * inv_step).ceil() as usize + 1).max(4);
    let ny = (((max_pt.y - min_pt.y) * inv_step).ceil() as usize + 1).max(4);
    let nz = (((max_pt.z - min_pt.z) * inv_step).ceil() as usize + 1).max(4);
    let stride_y = nz;
    let stride_x = ny * nz;
    let total_pts = nx * ny * nz;

    let mut phi_a = vec![10.0_f32; total_pts];
    let mut phi_b = vec![10.0_f32; total_pts];
    let mut nearest_atom_grid = vec![nodes[0].global_atom; total_pts];
    let mut best_dist_grid = vec![f32::INFINITY; total_pts];

    // 2. Splat both sides onto separate smooth-union signed distance fields.
    let mut splat_atoms = |atoms: &[(Vec3, f32, GlobalAtomIdx)], phi: &mut [f32]| {
        for &(center, r_i, g_atom) in atoms {
            let reach = r_i + p.void_max_gap_a + step * 1.5;
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
                        phi[g_idx] = smin_poly(phi[g_idx], d_surf, p.void_smooth_k_a);
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

    // 3. Evaluate psi(p) = max(-min(phi_a, phi_b), max(phi_a, phi_b) - gap_cutoff) and extract mesh.
    let mut psi = vec![10.0_f32; total_pts];
    for g in 0..total_pts {
        let da = phi_a[g];
        let db = phi_b[g];
        psi[g] = (-da.min(db)).max(da.max(db) - p.void_max_gap_a);
    }

    extract_dual_surface_net(
        &psi,
        nx,
        ny,
        nz,
        min_pt,
        step,
        &nearest_atom_grid,
        &best_dist_grid,
        nodes[0].global_atom,
    )
}
