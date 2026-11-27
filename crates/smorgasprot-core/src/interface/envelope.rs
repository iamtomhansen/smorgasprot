//! Two-sided interface contact envelope mesh and Lawrence & Colman (1993) shape complementarity.
//!
//! ## Summary
//! Extracts the two-sided molecular skin boundary between `SideA` and `SideB` from per-chain
//! surface meshes ([`extract_interface_envelope_mesh`]) and quantifies geometric packing via
//! Lawrence & Colman (1993) shape complementarity ([`compute_shape_complementarity`]).
//!
//! ## Considerations
//! - **Shape complementarity (`S_c`)**: For each interfacial surface vertex on `SideA`, finds
//!   the nearest `SideB` surface vertex and scores `S = (n_A · (-n_B)) * exp(-w * d²)`, with
//!   `w = 0.5 Å⁻²` (Lawrence & Colman, 1993), and vice versa for `SideB → SideA`. `S_c` is the
//!   median of the combined `S` values (typically `0.64–0.75` for well-packed protein interfaces).
//! - **Triangle inclusion**: [`extract_interface_envelope_mesh`] includes any chain-surface
//!   triangle with at least one vertex touching an interfacial atom.
//!
//! ## References
//! - Connolly, M. L. (1986). Shape complementarity in protein-protein interfaces.
//!   https://doi.org/10.1002/bip.360250705
//! - Lawrence, M. C., Colman, P. M. (1993). Shape complementarity at protein/protein interfaces.
//!   https://doi.org/10.1006/jmbi.1993.1648
//!
//! ## TODO
//! - Add a spatial grid acceleration if `envelope_mesh` vertex counts exceed a few thousand.

use super::definition::{InterfacePartition, InterfaceSide};
use super::model::AtomNode;
use crate::model::{ComplexStructure, SurfaceMesh};
use glam::Vec3;

/// Extracts the 3D two-sided interface contact envelope mesh by duplicating in-place the contact
/// surface triangles from `SideA` and `SideB` chain surface meshes that touch interfacial atoms.
pub fn extract_interface_envelope_mesh(
    chain_meshes: &[SurfaceMesh],
    complex: &ComplexStructure,
    partition: &InterfacePartition,
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
    let atom_sides = partition.atom_sides(complex);

    for (c_idx, chain_mesh) in chain_meshes.iter().enumerate() {
        if c_idx >= complex.chains.len() {
            continue;
        }
        let chain = &complex.chains[c_idx];
        let has_assigned = (chain.atom_range.0..chain.atom_range.1)
            .any(|a| atom_sides.get(a as usize).is_some_and(|s| s.is_assigned()));
        if !has_assigned {
            continue;
        }

        let mut local_remap = vec![u32::MAX; chain_mesh.vertices.len()];

        for &[i0, i1, i2] in &chain_mesh.triangles {
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
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

            if touch_count >= 1 {
                let get_or_insert = |v_idx: usize,
                                     remap: &mut [u32],
                                     mesh: &SurfaceMesh,
                                     out: &mut SurfaceMesh|
                 -> u32 {
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

/// Computes Lawrence & Colman (1993) shape complementarity `S_c ∈ [-1, 1]` on `envelope`.
///
/// Uses the canonical Gaussian distance weight `w = 0.5 Å⁻²` and returns the median of
/// `S = (n_A · (-n_B)) * exp(-w * d²)` across `SideA → SideB` and `SideB → SideA` vertex pairs.
pub fn compute_shape_complementarity(
    envelope: &SurfaceMesh,
    complex: &ComplexStructure,
    partition: &InterfacePartition,
) -> f32 {
    const W_DECAY: f32 = 0.5;
    if envelope.vertices.is_empty() {
        return 0.0;
    }

    // 1. Partition envelope vertices into SideA and SideB using their nearest atom.
    let atom_sides = partition.atom_sides(complex);
    let mut side_a: Vec<(Vec3, Vec3)> = Vec::new();
    let mut side_b: Vec<(Vec3, Vec3)> = Vec::new();

    for i in 0..envelope.vertices.len() {
        let a_idx = envelope.nearest_atom.get(i).map_or(usize::MAX, |g| g.0 as usize);
        let side = atom_sides.get(a_idx).copied().unwrap_or(InterfaceSide::None);
        let pos = Vec3::from(envelope.vertices[i]);
        let norm = Vec3::from(envelope.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]))
            .normalize_or_zero();
        match side {
            InterfaceSide::SideA => side_a.push((pos, norm)),
            InterfaceSide::SideB => side_b.push((pos, norm)),
            InterfaceSide::None => {}
        }
    }

    if side_a.is_empty() || side_b.is_empty() {
        return 0.0;
    }

    // 2. Score each vertex against its nearest opposing-side vertex.
    let mut scores = Vec::with_capacity(side_a.len() + side_b.len());
    let mut score_against = |src: &[(Vec3, Vec3)], dst: &[(Vec3, Vec3)]| {
        for &(pa, na) in src {
            let mut best_d2 = f32::INFINITY;
            let mut best_nb = Vec3::ZERO;
            for &(pb, nb) in dst {
                let d2 = pa.distance_squared(pb);
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_nb = nb;
                }
            }
            let s = na.dot(-best_nb) * (-W_DECAY * best_d2).exp();
            scores.push(s.clamp(-1.0, 1.0));
        }
    };

    score_against(&side_a, &side_b);
    score_against(&side_b, &side_a);

    // 3. Return the median of the combined S distribution.
    scores.sort_unstable_by(|a, b| a.total_cmp(b));
    let mid = scores.len() / 2;
    if scores.len() % 2 == 1 {
        scores[mid]
    } else {
        0.5 * (scores[mid - 1] + scores[mid])
    }
}
