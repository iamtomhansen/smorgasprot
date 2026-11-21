//! Shared projection-plane singular value decomposition (SVD) basis, coupled 2D spring/repulsion
//! solver, and bas-relief/butterfly kinematics.

use crate::contour::compute_territory_contours;
use crate::model::{
    AtomNode, ComplexStructure, EdgeKind, InterfaceEdge, InterfaceMap, InterfaceSide,
    ProjectionPlane, ResidueTerritory,
};
use crate::params::AnalysisParams;
use crate::physics::extract_3d_interface_graph;
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

crate::param_group! {
    /// Coupled 2D spring/repulsion solver that unrolls the interface onto the tangent plane.
    pub struct RelaxParams("relax", "2D Map Relaxation") {
        /// Solver iterations.
        iterations: usize = 160, 0..=2000, "Solver iterations", "";
        /// Initial integration step.
        initial_step: f32 = 0.08, 0.001..=0.5, "Initial step size", "";
        /// Step multiplier applied over the schedule.
        step_decay: f32 = 0.65, 0.1..=1.0, "Step decay rate", "";
        /// Maximum per-iteration node displacement (Å).
        max_step_a: f32 = 0.45, 0.05..=2.0, "Max displacement per step", "Å";
        /// Spring stiffness between atoms of the same residue.
        k_spring_same_res: f32 = 1.40, 0.0..=5.0, "Same-residue spring stiffness", "";
        /// Spring stiffness for intra-side polar contacts.
        k_spring_intra_polar: f32 = 0.90, 0.0..=5.0, "Intra-polar spring stiffness", "";
        /// Spring stiffness for other first-shell neighbors.
        k_spring_first_shell: f32 = 0.45, 0.0..=5.0, "First-shell spring stiffness", "";
        /// Registration stiffness pulling polar cross-interface partners together.
        k_reg_cross_polar: f32 = 0.30, 0.0..=2.0, "Cross-polar registration stiffness", "";
        /// Registration stiffness for apolar cross-interface partners.
        k_reg_cross_apolar: f32 = 0.10, 0.0..=2.0, "Cross-apolar registration stiffness", "";
        /// Cross partners closer than this in 2D feel no registration force (Å).
        cross_reg_deadzone_a: f32 = 1.20, 0.0..=5.0, "Cross registration deadzone", "Å";
        /// Target 2D separation for registered cross partners (Å).
        cross_reg_target_a: f32 = 0.80, 0.0..=5.0, "Cross registration target distance", "Å";
        /// Hard-core repulsion stiffness between same-side atoms.
        k_repel_hard_core: f32 = 2.20, 0.0..=10.0, "Hard-core repulsion stiffness", "";
        /// Minimum 2D separation for atoms of the same residue (Å).
        min_dist_same_res_a: f32 = 1.45, 0.5..=4.0, "Same-residue min separation", "Å";
        /// Minimum 2D separation for atoms of different residues (Å).
        min_dist_diff_res_a: f32 = 2.15, 0.5..=4.0, "Diff-residue min separation", "Å";
        /// 2D distance below which a pair is reported as an overlap (Å).
        overlap_diagnostic_dist_a: f32 = 1.90, 0.5..=4.0, "Overlap diagnostic threshold", "Å";
        /// Pull of each atom toward its residue's 2D centroid.
        k_territory_cohesion: f32 = 0.18, 0.0..=2.0, "Territory centroid cohesion", "";
        /// Tether of each atom to its initial tangent-plane projection.
        k_anchor_tether: f32 = 0.08, 0.0..=2.0, "Initial tangent-plane tether", "";
    }
}

/// Diagnostics summarizing the quality of the 3D -> 2D metric unwrapping.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct FlatteningStats {
    pub side_a_atoms: usize,
    pub side_b_atoms: usize,
    pub side_a_territories: usize,
    pub side_b_territories: usize,
    pub intra_edges: usize,
    pub cross_contacts: usize,
    pub edge_rmse_before_a: f32,
    /// Intra-side 1st-shell edge RMSE at `flatten_t = 0.5` (3D bas-relief intermediate).
    pub edge_rmse_halfway_a: f32,
    /// Intra-side 1st-shell edge RMSE at `flatten_t = 1.0` (fully flat 2D map).
    pub edge_rmse_after_a: f32,
    pub overlaps_before: usize,
    pub overlaps_after: usize,
}

/// Extracts the dual-side 3D interface graph via [`extract_3d_interface_graph`], computes the shared
/// SVD [`ProjectionPlane`], relaxes the 2D coordinates `(u, v)`, builds ΔSASA-weighted Voronoi
/// residue territory contours, and generates the 3D inter-chain interface separator sheet mesh.
pub fn build_interface_map(
    complex: &ComplexStructure,
    params: &AnalysisParams,
) -> Result<(InterfaceMap, FlatteningStats), String> {
    // 1. Extract the 3D biophysical interface graph (SASA, aliphatic scores, classified 3D edges)
    let (mut territories, mut nodes, mut edges) =
        extract_3d_interface_graph(complex, &params.sasa, &params.contacts)?;

    let cross_contacts_count = edges.iter().filter(|e| e.is_cross_interface).count();
    let intra_edges_count = edges.len() - cross_contacts_count;

    // 2. Compute shared SVD projection plane Π = (origin, axis_u, axis_v, normal)
    let mut plane = compute_shared_tangent_plane(&nodes);

    // 4. Project atoms onto initial (u^(0), v^(0)) coordinates
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);
    let mut initial_uv = Vec::with_capacity(nodes.len());

    for node in &mut nodes {
        let rel = Vec3::from(node.pos_3d) - origin;
        let uv = [rel.dot(axis_u), rel.dot(axis_v)];
        node.pos_uv_raw = uv;
        node.pos_uv = uv;
        initial_uv.push(Vec2::from(uv));
    }

    let edge_rmse_before_a = compute_intra_edge_rmse_2d(&nodes, &edges);
    let overlaps_before = count_same_side_overlaps(&nodes, params.relax.overlap_diagnostic_dist_a);

    // 5. Run coupled 2D spring + hard-core repulsion + contact registration relaxation
    relax_interface_uv(&mut nodes, &territories, &edges, &initial_uv, &params.relax);

    let min_u = nodes
        .iter()
        .map(|n| n.pos_uv[0])
        .fold(f32::INFINITY, f32::min);
    plane.hinge_u = min_u - 3.5;

    // Record final 2D edge length and signed metric strain (dist_2d - dist_3d) on every InterfaceEdge
    for edge in &mut edges {
        let pa = Vec2::from(nodes[edge.a.0 as usize].pos_uv);
        let pb = Vec2::from(nodes[edge.b.0 as usize].pos_uv);
        let d2d = pa.distance(pb);
        edge.dist_2d_a = d2d;
        edge.strain_2d_a = d2d - edge.dist_3d_a;
    }

    let edge_rmse_bas_relief_a = compute_intra_edge_rmse_at_t(&nodes, &edges, &plane, 0.5);
    let edge_rmse_after_a = compute_intra_edge_rmse_2d(&nodes, &edges);
    let overlaps_after = count_same_side_overlaps(&nodes, params.relax.overlap_diagnostic_dist_a);

    // 6. Compute Voronoi-clipped smooth concave residue territory contours & 3D relief outlines
    compute_territory_contours(&mut territories, &nodes, &plane, &params.contour);

    let side_a_atoms = nodes
        .iter()
        .filter(|n| n.role == InterfaceSide::SideA)
        .count();
    let side_b_atoms = nodes.len() - side_a_atoms;
    let side_a_territories = territories
        .iter()
        .filter(|t| t.role == InterfaceSide::SideA)
        .count();
    let side_b_territories = territories.len() - side_a_territories;

    let stats = FlatteningStats {
        side_a_atoms,
        side_b_atoms,
        side_a_territories,
        side_b_territories,
        intra_edges: intra_edges_count,
        cross_contacts: cross_contacts_count,
        edge_rmse_before_a,
        edge_rmse_halfway_a: edge_rmse_bas_relief_a,
        edge_rmse_after_a,
        overlaps_before,
        overlaps_after,
    };

    let map = InterfaceMap {
        plane,
        territories,
        nodes,
        edges,
    };

    Ok((map, stats))
}

/// Computes the weighted SVD best-fit projection plane for the interface patch,
/// with normal pointing from SideB centroid toward SideA centroid.
fn compute_shared_tangent_plane(nodes: &[AtomNode]) -> ProjectionPlane {
    let mut weighted_sum = Vec3::ZERO;
    let mut total_w = 0.0_f32;
    let mut side_a_sum = Vec3::ZERO;
    let mut side_a_count = 0usize;
    let mut side_b_sum = Vec3::ZERO;
    let mut side_b_count = 0usize;

    for n in nodes {
        let p = Vec3::from(n.pos_3d);
        let w = n.buried_sasa_a2 + 1.0;
        weighted_sum += w * p;
        total_w += w;
        match n.role {
            InterfaceSide::SideA => {
                side_a_sum += p;
                side_a_count += 1;
            }
            InterfaceSide::SideB => {
                side_b_sum += p;
                side_b_count += 1;
            }
            InterfaceSide::None => {}
        }
    }

    let origin = if total_w > 0.0 {
        weighted_sum / total_w
    } else {
        Vec3::ZERO
    };

    let mut cov = [[0.0_f32; 3]; 3];
    for n in nodes {
        let d = Vec3::from(n.pos_3d) - origin;
        let w = n.buried_sasa_a2 + 1.0;
        let arr = d.to_array();
        for r in 0..3 {
            for c in 0..3 {
                cov[r][c] += w * arr[r] * arr[c];
            }
        }
    }

    let (eig_vals, eig_vecs) = jacobi_eigen_3x3(cov);

    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| {
        eig_vals[b]
            .partial_cmp(&eig_vals[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let axis_u = Vec3::from(eig_vecs[order[0]]).normalize_or_zero();
    let mut axis_v = Vec3::from(eig_vecs[order[1]]).normalize_or_zero();
    let mut normal = Vec3::from(eig_vecs[order[2]]).normalize_or_zero();

    // Orient normal from SideB -> SideA
    if side_a_count > 0 && side_b_count > 0 {
        let side_a_centroid = side_a_sum / (side_a_count as f32);
        let side_b_centroid = side_b_sum / (side_b_count as f32);
        if normal.dot(side_a_centroid - side_b_centroid) < 0.0 {
            normal = -normal;
        }
    }

    if axis_u.cross(axis_v).dot(normal) < 0.0 {
        axis_v = -axis_v;
    }

    ProjectionPlane {
        origin: origin.to_array(),
        axis_u: axis_u.to_array(),
        axis_v: axis_v.to_array(),
        normal: normal.to_array(),
        hinge_u: -15.0,
    }
}

/// Analytic 3x3 symmetric Jacobi eigenvalue solver.
fn jacobi_eigen_3x3(mut a: [[f32; 3]; 3]) -> ([f32; 3], [[f32; 3]; 3]) {
    let mut v = [[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    for _ in 0..25 {
        let mut p = 0;
        let mut q = 1;
        let mut max_val = a[0][1].abs();
        if a[0][2].abs() > max_val {
            p = 0;
            q = 2;
            max_val = a[0][2].abs();
        }
        if a[1][2].abs() > max_val {
            p = 1;
            q = 2;
            max_val = a[1][2].abs();
        }
        if max_val < 1e-7 {
            break;
        }

        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        let tau = (aqq - app) / (2.0 * apq);
        let t = if tau >= 0.0 {
            1.0 / (tau + (1.0 + tau * tau).sqrt())
        } else {
            -1.0 / (-tau + (1.0 + tau * tau).sqrt())
        };
        let c = 1.0 / (1.0 + t * t).sqrt();
        let s = t * c;

        a[p][p] = app - t * apq;
        a[q][q] = aqq + t * apq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;

        for r in 0..3 {
            if r != p && r != q {
                let arp = a[r][p];
                let arq = a[r][q];
                a[r][p] = c * arp - s * arq;
                a[p][r] = a[r][p];
                a[r][q] = s * arp + c * arq;
                a[q][r] = a[r][q];
            }
            let vrp = v[r][p];
            let vrq = v[r][q];
            v[r][p] = c * vrp - s * vrq;
            v[r][q] = s * vrp + c * vrq;
        }
    }

    let vals = [a[0][0], a[1][1], a[2][2]];
    let vecs = [
        [v[0][0], v[1][0], v[2][0]],
        [v[0][1], v[1][1], v[2][1]],
        [v[0][2], v[1][2], v[2][2]],
    ];
    (vals, vecs)
}

/// Iterative damped spring + hard-core circle repulsion + cross-contact registration solver to
/// flatten nodes onto a shared 2D plane while preserving first-shell 3D distances.
/// 
/// Uses different spring functions that work together to preserve 3D atomic distances while
/// preventing overlaps and maintaining cross-interface registration of interacting pairs.
fn relax_interface_uv(
    nodes: &mut [AtomNode],
    territories: &[ResidueTerritory],
    edges: &[InterfaceEdge],
    initial_uv: &[Vec2],
    p: &RelaxParams,
) {
    let n = nodes.len();
    let mut pos: Vec<Vec2> = nodes.iter().map(|node| Vec2::from(node.pos_uv)).collect();
    let mut forces = vec![Vec2::ZERO; n];

    for iter in 0..p.iterations {
        let progress = (iter as f32) / (p.iterations as f32);
        let step_size = p.initial_step * (1.0 - p.step_decay * progress);
        forces.fill(Vec2::ZERO);

        // 1. Intra-side first-shell metric springs & cross-interface contact registration springs
        for edge in edges {
            let i = edge.a.0 as usize;
            let j = edge.b.0 as usize;
            let delta = pos[j] - pos[i];
            let dist = delta.length().max(1e-4);

            if !edge.is_cross_interface {
                // TODO: Handle covalently bonded antecedent heavy atoms (e.g. disulfide, glycan links).
                let k = match edge.kind {
                    EdgeKind::SameResidue => p.k_spring_same_res,
                    EdgeKind::HBond | EdgeKind::SaltBridge => p.k_spring_intra_polar,
                    EdgeKind::Clash | EdgeKind::VanDerWaalsContact => p.k_spring_first_shell,
                };
                let f = k * (dist - edge.dist_3d_a) * (delta / dist);
                forces[i] += f;
                forces[j] -= f;
            } else if dist > p.cross_reg_deadzone_a {
                let k_reg = match edge.kind {
                    EdgeKind::HBond | EdgeKind::SaltBridge | EdgeKind::Clash => p.k_reg_cross_polar,
                    EdgeKind::VanDerWaalsContact | EdgeKind::SameResidue => p.k_reg_cross_apolar,
                };
                let f = k_reg * (dist - p.cross_reg_target_a) * (delta / dist);
                forces[i] += f;
                forces[j] -= f;
            }
        }

        // 2. Same-side hard-core non-overlap repulsion
        for i in 0..n {
            for j in (i + 1)..n {
                if nodes[i].role != nodes[j].role {
                    continue;
                }
                let d_min = if nodes[i].territory == nodes[j].territory {
                    p.min_dist_same_res_a
                } else {
                    p.min_dist_diff_res_a
                };
                let delta = pos[j] - pos[i];
                let dist_sq = delta.length_squared();
                if dist_sq < d_min * d_min {
                    let dist = dist_sq.sqrt().max(1e-4);
                    let dir = if dist > 1e-3 {
                        delta / dist
                    } else {
                        Vec2::new(
                            ((i * 7 + j * 13) as f32).cos(),
                            ((i * 11 + j * 17) as f32).sin(),
                        )
                    };
                    let f = p.k_repel_hard_core * (d_min - dist) * dir;
                    forces[i] -= f;
                    forces[j] += f;
                }
            }
        }

        // 3. Residue territory cohesion
        for terr in territories {
            if terr.patch_atoms.len() <= 1 {
                continue;
            }
            let centroid = terr
                .patch_atoms
                .iter()
                .map(|&idx| pos[idx.0 as usize])
                .sum::<Vec2>()
                / (terr.patch_atoms.len() as f32);
            for &idx in &terr.patch_atoms {
                let i = idx.0 as usize;
                forces[i] += p.k_territory_cohesion * (centroid - pos[i]);
            }
        }

        // 4. Gentle anchor tether to projection-plane initial coordinates
        for i in 0..n {
            forces[i] += p.k_anchor_tether * (initial_uv[i] - pos[i]);
        }

        for i in 0..n {
            let disp = (step_size * forces[i]).clamp_length_max(p.max_step_a);
            pos[i] += disp;
        }
    }

    for (i, node) in nodes.iter_mut().enumerate() {
        node.pos_uv = pos[i].to_array();
    }
}

/// Root mean square error (RMSE) of intra-side 1st-shell edge lengths in 2D (`flatten_t = 1.0`) vs. true 3D.
pub fn compute_intra_edge_rmse_2d(nodes: &[AtomNode], edges: &[InterfaceEdge]) -> f32 {
    let mut sum_sq = 0.0_f32;
    let mut count = 0usize;
    for e in edges {
        if e.is_cross_interface {
            continue;
        }
        let pa = Vec2::from(nodes[e.a.0 as usize].pos_uv);
        let pb = Vec2::from(nodes[e.b.0 as usize].pos_uv);
        let err = pa.distance(pb) - e.dist_3d_a;
        sum_sq += err * err;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        (sum_sq / (count as f32)).sqrt()
    }
}

/// Dynamically computes the intra-side 1st-shell edge length RMSE at any continuous
/// flattening parameter `flatten_t` $\in [0, 1]$ (e.g. `0.0` = 3D, `0.5` = bas-relief, `1.0` = 2D).
pub fn compute_intra_edge_rmse_at_t(
    nodes: &[AtomNode],
    edges: &[InterfaceEdge],
    plane: &ProjectionPlane,
    flatten_t: f32,
) -> f32 {
    let mut sum_sq = 0.0_f32;
    let mut count = 0usize;
    for e in edges {
        if e.is_cross_interface {
            continue;
        }
        let na = &nodes[e.a.0 as usize];
        let nb = &nodes[e.b.0 as usize];
        let pa = Vec3::from(interpolate_atom_position(
            na.pos_3d, na.pos_uv, na.role, plane, flatten_t, 0.0,
        ));
        let pb = Vec3::from(interpolate_atom_position(
            nb.pos_3d, nb.pos_uv, nb.role, plane, flatten_t, 0.0,
        ));
        let err = pa.distance(pb) - e.dist_3d_a;
        sum_sq += err * err;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        (sum_sq / (count as f32)).sqrt()
    }
}

/// Counts the number of same-side node pairs from different residues that are closer than `min_dist` in 2D `(u, v)` space.
fn count_same_side_overlaps(nodes: &[AtomNode], min_dist: f32) -> usize {
    let mut count = 0;
    let min_d2 = min_dist * min_dist;
    for i in 0..nodes.len() {
        let pi = Vec2::from(nodes[i].pos_uv);
        for j in (i + 1)..nodes.len() {
            if nodes[i].role != nodes[j].role || nodes[i].territory == nodes[j].territory {
                continue;
            }
            let pj = Vec2::from(nodes[j].pos_uv);
            if pi.distance_squared(pj) < min_d2 {
                count += 1;
            }
        }
    }
    count
}

/// Computes the current 3D world position of an interfacial atom given:
/// - `pos_3d`: true 3D position
/// - `pos_uv`: relaxed 2D projection-plane coordinates `(u, v)`
/// - `side`: `InterfaceSide::SideA` or `InterfaceSide::SideB`
/// - `plane`: shared interface [`ProjectionPlane`]
/// - `flatten_t`: `0.0` (true 3D) -> `0.5` (3D bas-relief) -> `1.0` (flat 2D sheet)
/// - `book_theta`: `0.0` (overlaid) -> `1.0` (180-degree open-book butterfly)
pub fn interpolate_atom_position(
    pos_3d: [f32; 3],
    pos_uv: [f32; 2],
    side: InterfaceSide,
    plane: &ProjectionPlane,
    flatten_t: f32,
    book_theta: f32,
) -> [f32; 3] {
    let p3 = Vec3::from(pos_3d);
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);
    let normal = Vec3::from(plane.normal);

    let (u, v) = (pos_uv[0], pos_uv[1]);
    let sheet_offset = 0.05_f32;

    let p_flat = match side {
        InterfaceSide::SideA => origin + u * axis_u + v * axis_v + sheet_offset * normal,
        InterfaceSide::SideB => {
            let phi = book_theta.clamp(0.0, 1.0) * std::f32::consts::PI;
            let du = u - plane.hinge_u;
            let u_rot = plane.hinge_u + du * phi.cos();
            let n_rot = -du * phi.sin() - sheet_offset * phi.cos();
            origin + u_rot * axis_u + v * axis_v + n_rot * normal
        }
        InterfaceSide::None => p3,
    };

    let t = flatten_t.clamp(0.0, 1.0);
    p3.lerp(p_flat, t).to_array()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::load_complex;

    #[test]
    fn test_bas_relief_and_open_book_butterfly_interpolation() {
        let plane = ProjectionPlane {
            origin: [0.0, 0.0, 0.0],
            axis_u: [1.0, 0.0, 0.0],
            axis_v: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            hinge_u: -10.0,
        };

        let pos_3d = [4.0, 6.0, 8.0];
        let pos_uv = [2.0, 5.0];

        let p_3d =
            interpolate_atom_position(pos_3d, pos_uv, InterfaceSide::SideB, &plane, 0.0, 1.0);
        assert_eq!(p_3d, pos_3d);

        let p_side_a_overlay =
            interpolate_atom_position(pos_3d, pos_uv, InterfaceSide::SideA, &plane, 1.0, 0.0);
        let p_side_b_overlay =
            interpolate_atom_position(pos_3d, pos_uv, InterfaceSide::SideB, &plane, 1.0, 0.0);
        assert!((p_side_a_overlay[0] - 2.0).abs() < 1e-5);
        assert!((p_side_b_overlay[0] - 2.0).abs() < 1e-5);

        let p_side_b_book =
            interpolate_atom_position(pos_3d, pos_uv, InterfaceSide::SideB, &plane, 1.0, 1.0);
        assert!((p_side_b_book[0] - (-22.0)).abs() < 1e-4);
        assert!((p_side_b_book[1] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn test_flatten_7jvb_and_latentx() {
        let c_7jvb =
            load_complex("../../examples/7JVB.cif", &["A".to_string()], &["C".to_string()])
                .unwrap();
        assert_eq!(c_7jvb.chains.len(), 4);
        let (map_7jvb, stats_7jvb) = build_interface_map(&c_7jvb, &Default::default()).unwrap();
        println!("7JVB stats: {stats_7jvb:?}");
        assert!(stats_7jvb.side_a_atoms > 30);
        assert!(stats_7jvb.side_b_atoms > 30);
        assert!(stats_7jvb.overlaps_after < stats_7jvb.overlaps_before);
        // Verify that bas-relief (t = 0.5) has even lower edge distortion than full 2D (t = 1.0)
        assert!(stats_7jvb.edge_rmse_halfway_a < stats_7jvb.edge_rmse_after_a);
        assert!(!map_7jvb.territories[0].contour_uv.is_empty());
        for t in &map_7jvb.territories {
            assert_eq!(t.contour_uv.len(), t.contour_3d.len());
            assert!(t.contour_uv.len() >= 12);
        }
        assert!(!map_7jvb.nodes.is_empty());
        assert!(!map_7jvb.edges.is_empty());

        let c_lx = load_complex(
            "../../examples/fold_r8s15_mp3_model_0.cif",
            &["B".to_string()],
            &["A".to_string()],
        )
        .unwrap();
        let (_map_lx, stats_lx) = build_interface_map(&c_lx, &Default::default()).unwrap();
        println!("LatentX stats: {stats_lx:?}");
        assert!(stats_lx.side_a_atoms > 20);
        assert!(stats_lx.side_b_atoms > 20);
        assert!(stats_lx.edge_rmse_halfway_a < stats_lx.edge_rmse_after_a);
    }
}
