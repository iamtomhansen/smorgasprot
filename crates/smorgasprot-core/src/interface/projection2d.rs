//! Flatten the interfacial atoms onto a shared 2D projection plane while preserving local
//! chemical interactions and distances.
//!
//! ## Summary
//! Inspecting interfacial interactions in 3D can be frustrating, especially for large and complex
//! surfaces, and the amount of data that can be shown in 3D is limited. Instead, both halves of
//! that interface can be flattened onto a shared 2D plane while preserving local features.
//! The primary entry point is [`build_interface_map`], which outputs an [`InterfaceMap`] and
//! [`FlatteningStats`] (used to assess distortion upon flattening). [`interpolate_atom_position`]
//! allows partial flattening by simple linear interpolation of 3D and 2D coordinates.
//!
//! ## Considerations
//! - **Projection plane**: [`compute_shared_tangent_plane`] fits a SASA-weighted 3×3 covariance
//!   basis (`buried_sasa_a2 + 1.0`) via symmetric Jacobi eigendecomposition, oriented `SideB → SideA`.
//! - **Coupled relaxation**: `SideA` and `SideB` relax simultaneously in `(u, v)` with intra-side
//!   metric springs, same-side hard-core repulsion, cross-interface registration springs, residue
//!   cohesion, and a gentle anchor tether.
//! - **Determinism**: Exact 2D overlaps are broken with deterministic index-seeded angles; all
//!   loops iterate in slice order.
//!
//! ## TODO
//! - Consider a dedicated registration stiffness for cross-interface covalent/disulfide links.

use super::contour::compute_territory_contours;
use super::definition::{InterfacePartition, InterfaceSide};
use super::model::{
    AtomNode, EdgeKind, InterfaceEdge, InterfaceMap, ProjectionPlane, ResidueTerritory,
};
use super::uv_mesh::build_interface_sheet_mesh;
use crate::model::{ComplexStructure, SurfaceMesh};
use crate::params::AnalysisParams;
use crate::physics::extract_3d_interface_graph;
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

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
/// weighted-PCA [`ProjectionPlane`], relaxes the 2D coordinates `(u, v)`, and builds ΔSASA-weighted
/// Voronoi residue territory contours.
pub fn build_interface_map(
    complex: &ComplexStructure,
    partition: &InterfacePartition,
    params: &AnalysisParams,
) -> Result<(InterfaceMap, FlatteningStats), String> {
    // 1. Extract the 3D biophysical interface graph (SASA, aliphatic scores, classified 3D edges)
    let (mut territories, mut nodes, mut edges) =
        extract_3d_interface_graph(complex, partition, &params.sasa, &params.contacts)?;

    let cross_contacts_count = edges.iter().filter(|e| e.is_cross_interface).count();
    let intra_edges_count = edges.len() - cross_contacts_count;

    // 2. Compute shared weighted-PCA projection plane Π = (origin, axis_u, axis_v, normal)
    let plane = compute_projection_plane(&nodes);

    // 3. Build curved 3D interface sheet (uv_mesh) and unroll atoms onto initial (u^(0), v^(0))
    //    using geodesic arc-lengths along the sheet (falls back to flat PCA if empty).
    let sheet_mesh =
        build_interface_sheet_mesh(complex, &nodes, &plane, &params.interface_mesh);
    let initial_uv = project_atoms_onto_manifold(&mut nodes, &plane, &sheet_mesh);

    let edge_rmse_before_a = compute_intra_edge_rmse_2d(&nodes, &edges);
    let overlaps_before = count_same_side_overlaps(&nodes, params.relax.overlap_diagnostic_dist_a);

    // 4. Run coupled 2D spring + hard-core repulsion + contact registration relaxation
    relax_interface_uv(&mut nodes, &territories, &edges, &initial_uv, &params.relax);

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

    // 5. Compute Voronoi-clipped smooth concave residue territory contours draped onto sheet_mesh
    let sheet_flat_uv = compute_sheet_flat_uv(&sheet_mesh, &plane);
    let sheet_ref = (!sheet_mesh.vertices.is_empty() && !sheet_mesh.triangles.is_empty())
        .then_some((&sheet_mesh, sheet_flat_uv.as_slice()));
    compute_territory_contours(&mut territories, &nodes, &plane, sheet_ref, &params.contour);

    let side_a_atoms = nodes
        .iter()
        .filter(|n| n.side == InterfaceSide::SideA)
        .count();
    let side_b_atoms = nodes.len() - side_a_atoms;
    let side_a_territories = territories
        .iter()
        .filter(|t| t.side == InterfaceSide::SideA)
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

#[derive(Copy, Clone, PartialEq)]
struct GeoState {
    dist: f32,
    idx: usize,
}

impl Eq for GeoState {}

impl Ord for GeoState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: smaller distance pops first, deterministic index tie-break.
        other.dist.total_cmp(&self.dist).then_with(|| other.idx.cmp(&self.idx))
    }
}

impl PartialOrd for GeoState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Aggregate strain and surface-area diagnostics for a morphed 3D ↔ 2D interface sheet.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct SheetFlatteningStats {
    /// Root-mean-square edge-length distortion `sqrt(mean((L_t - L_3d)^2))` (Å).
    pub rms_edge_strain_a: f32,
    /// Mean absolute relative linear strain `mean(|L_t - L_3d| / L_3d)` (dimensionless).
    pub mean_abs_strain: f32,
    /// Worst-case relative linear strain `max(|L_t - L_3d| / L_3d)` (dimensionless).
    pub max_abs_strain: f32,
    /// Surface area of the unflattened 3D interface sheet (Å²).
    pub area_3d_a2: f32,
    /// Surface area of the sheet at the current `flatten_t` (Å²).
    pub area_current_a2: f32,
    /// Half-total buried SASA (`0.5 * Σ ΔSASA`) for reference against `area_3d_a2` (Å²).
    pub buried_sasa_a2: f32,
}

/// Morphed world-space interface sheet at a given `flatten_t`, with per-vertex signed strain.
#[derive(Clone, Debug, Default)]
pub struct MorphedSheet {
    pub mesh: SurfaceMesh,
    /// Signed per-vertex relative linear strain (`< 0` compressed, `0` isometric, `> 0` stretched).
    pub vertex_strain: Vec<f32>,
    pub stats: SheetFlatteningStats,
}

/// Computes geodesic-unrolled 2D `(u, v)` rest coordinates for every vertex of `sheet_mesh`
/// using Dijkstra arc-lengths from the vertex closest to `plane.origin`.
pub fn compute_sheet_flat_uv(sheet_mesh: &SurfaceMesh, plane: &ProjectionPlane) -> Vec<[f32; 2]> {
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);
    let flat_uv = |pos: Vec3| -> Vec2 {
        let rel = pos - origin;
        Vec2::new(rel.dot(axis_u), rel.dot(axis_v))
    };

    let n_verts = sheet_mesh.vertices.len();
    if n_verts == 0 {
        return Vec::new();
    }
    let vert_pos: Vec<Vec3> = sheet_mesh.vertices.iter().map(|&v| Vec3::from(v)).collect();
    if sheet_mesh.triangles.is_empty() {
        return vert_pos.iter().map(|&p| flat_uv(p).to_array()).collect();
    }

    // 1. Build 3D edge-weighted adjacency list on sheet_mesh.
    let mut adj: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n_verts];
    for &[a, b, c] in &sheet_mesh.triangles {
        let (ua, ub, uc) = (a as usize, b as usize, c as usize);
        if ua < n_verts && ub < n_verts && uc < n_verts {
            let dab = vert_pos[ua].distance(vert_pos[ub]);
            let dbc = vert_pos[ub].distance(vert_pos[uc]);
            let dca = vert_pos[uc].distance(vert_pos[ua]);
            adj[ua].push((ub, dab));
            adj[ub].push((ua, dab));
            adj[ub].push((uc, dbc));
            adj[uc].push((ub, dbc));
            adj[uc].push((ua, dca));
            adj[ua].push((uc, dca));
        }
    }

    // 2. Seed Dijkstra at the sheet vertex closest to plane.origin.
    let seed = vert_pos
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.distance_squared(origin).total_cmp(&b.distance_squared(origin)))
        .map_or(0, |(i, _)| i);
    let seed_uv = flat_uv(vert_pos[seed]);

    let mut geo_dist = vec![f32::INFINITY; n_verts];
    let mut heap = BinaryHeap::new();
    geo_dist[seed] = 0.0;
    heap.push(GeoState { dist: 0.0, idx: seed });

    while let Some(GeoState { dist, idx }) = heap.pop() {
        if dist > geo_dist[idx] {
            continue;
        }
        for &(nbr, w) in &adj[idx] {
            let cand = dist + w;
            if cand < geo_dist[nbr] {
                geo_dist[nbr] = cand;
                heap.push(GeoState { dist: cand, idx: nbr });
            }
        }
    }

    // 3. Map each vertex to (u, v) along its PCA direction scaled by geodesic arc-length.
    vert_pos
        .iter()
        .enumerate()
        .map(|(vi, &pos)| {
            let raw = flat_uv(pos);
            if geo_dist[vi].is_finite() {
                let dir = (raw - seed_uv).normalize_or_zero();
                if dir.length_squared() > 0.5 {
                    (seed_uv + dir * geo_dist[vi]).to_array()
                } else {
                    raw.to_array()
                }
            } else {
                raw.to_array()
            }
        })
        .collect()
}

/// Morphs `sheet_mesh` from its 3D shape (`flatten_t = 0.0`) toward its flattened resting place
/// on `plane` (`flatten_t = 1.0`), dynamically computing per-vertex signed strain and aggregate
/// distortion/area diagnostics.
pub fn morph_sheet_with_strain(
    sheet_mesh: &SurfaceMesh,
    flat_uv: &[[f32; 2]],
    plane: &ProjectionPlane,
    flatten_t: f32,
    buried_sasa_a2: f32,
) -> MorphedSheet {
    let n_verts = sheet_mesh.vertices.len();
    if n_verts == 0 || flat_uv.len() != n_verts {
        return MorphedSheet::default();
    }

    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);
    let t = flatten_t.clamp(0.0, 1.0);

    // 1. Lerp vertices in world space toward their flat resting coordinates on `plane`.
    let mut pos_3d = Vec::with_capacity(n_verts);
    let mut pos_t = Vec::with_capacity(n_verts);
    for (i, &v) in sheet_mesh.vertices.iter().enumerate() {
        let p3 = Vec3::from(v);
        let [u, v_coord] = flat_uv[i];
        let p_flat = origin + u * axis_u + v_coord * axis_v;
        pos_3d.push(p3);
        pos_t.push(p3.lerp(p_flat, t));
    }

    // 2. Accumulate per-vertex area-weighted normals, triangle areas, and edge strain.
    let mut normal_acc = vec![Vec3::ZERO; n_verts];
    let mut strain_sum = vec![0.0_f32; n_verts];
    let mut strain_cnt = vec![0u32; n_verts];
    let mut area_3d = 0.0_f64;
    let mut area_cur = 0.0_f64;
    let mut edge_err_sq = 0.0_f64;
    let mut abs_rel_sum = 0.0_f64;
    let mut max_abs_strain = 0.0_f32;
    let mut edge_count = 0usize;

    for &[a, b, c] in &sheet_mesh.triangles {
        let (ua, ub, uc) = (a as usize, b as usize, c as usize);
        if ua >= n_verts || ub >= n_verts || uc >= n_verts {
            continue;
        }
        let n3 = (pos_3d[ub] - pos_3d[ua]).cross(pos_3d[uc] - pos_3d[ua]);
        area_3d += 0.5 * n3.length() as f64;

        let nt = (pos_t[ub] - pos_t[ua]).cross(pos_t[uc] - pos_t[ua]);
        area_cur += 0.5 * nt.length() as f64;
        normal_acc[ua] += nt;
        normal_acc[ub] += nt;
        normal_acc[uc] += nt;

        for (i0, i1) in [(ua, ub), (ub, uc), (uc, ua)] {
            let l3 = pos_3d[i0].distance(pos_3d[i1]);
            if l3 < 1e-6 {
                continue;
            }
            let lt = pos_t[i0].distance(pos_t[i1]);
            let diff = lt - l3;
            let rel = diff / l3;
            let abs_rel = rel.abs();

            edge_err_sq += (diff as f64) * (diff as f64);
            abs_rel_sum += abs_rel as f64;
            max_abs_strain = max_abs_strain.max(abs_rel);
            edge_count += 1;

            strain_sum[i0] += rel;
            strain_cnt[i0] += 1;
            strain_sum[i1] += rel;
            strain_cnt[i1] += 1;
        }
    }

    let vertex_strain: Vec<f32> = strain_sum
        .into_iter()
        .zip(strain_cnt)
        .map(|(s, c)| if c > 0 { s / c as f32 } else { 0.0 })
        .collect();

    let normals: Vec<[f32; 3]> = normal_acc
        .into_iter()
        .map(|n| n.normalize_or_zero().to_array())
        .collect();

    let vertices: Vec<[f32; 3]> = pos_t.iter().map(Vec3::to_array).collect();
    let stats = SheetFlatteningStats {
        rms_edge_strain_a: if edge_count > 0 {
            (edge_err_sq / edge_count as f64).sqrt() as f32
        } else {
            0.0
        },
        mean_abs_strain: if edge_count > 0 {
            (abs_rel_sum / edge_count as f64) as f32
        } else {
            0.0
        },
        max_abs_strain,
        area_3d_a2: area_3d as f32,
        area_current_a2: area_cur as f32,
        buried_sasa_a2,
    };

    MorphedSheet {
        mesh: SurfaceMesh {
            vertices,
            normals,
            nearest_atom: sheet_mesh.nearest_atom.clone(),
            triangles: sheet_mesh.triangles.clone(),
        },
        vertex_strain,
        stats,
    }
}

/// Unrolls atoms onto initial `(u^(0), v^(0))` coordinates using 3D geodesic arc-lengths along
/// `sheet_mesh` from the vertex closest to `plane.origin`, falling back to flat PCA projection
/// when `sheet_mesh` is empty.
fn project_atoms_onto_manifold(
    nodes: &mut [AtomNode],
    plane: &ProjectionPlane,
    sheet_mesh: &SurfaceMesh,
) -> Vec<Vec2> {
    let origin = Vec3::from(plane.origin);
    let axis_u = Vec3::from(plane.axis_u);
    let axis_v = Vec3::from(plane.axis_v);

    let flat_uv = |pos: Vec3| -> Vec2 {
        let rel = pos - origin;
        Vec2::new(rel.dot(axis_u), rel.dot(axis_v))
    };

    if sheet_mesh.vertices.is_empty() || sheet_mesh.triangles.is_empty() {
        return nodes
            .iter_mut()
            .map(|node| {
                let uv = flat_uv(Vec3::from(node.pos_3d));
                node.pos_uv_raw = uv.to_array();
                node.pos_uv = node.pos_uv_raw;
                uv
            })
            .collect();
    }

    let vert_pos: Vec<Vec3> = sheet_mesh.vertices.iter().map(|&v| Vec3::from(v)).collect();
    let sheet_uv = compute_sheet_flat_uv(sheet_mesh, plane);

    nodes
        .iter_mut()
        .map(|node| {
            let pos = Vec3::from(node.pos_3d);
            let best_v = vert_pos
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.distance_squared(pos).total_cmp(&b.distance_squared(pos)))
                .map(|(vi, _)| vi);

            let uv = best_v.map_or_else(|| flat_uv(pos), |vi| Vec2::from(sheet_uv[vi]));
            node.pos_uv_raw = uv.to_array();
            node.pos_uv = node.pos_uv_raw;
            uv
        })
        .collect()
}

/// Computes the weighted SVD best-fit projection plane for the interface patch,
/// with normal pointing from SideB centroid toward SideA centroid.
fn compute_projection_plane(nodes: &[AtomNode]) -> ProjectionPlane {
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
        match n.side {
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
                // Covalent bonds are physically stiff and short-range, so they all share the same
                // (stiffest) spring constant as same-residue geometry.
                let k = match edge.kind {
                    EdgeKind::Covalent(_) => p.k_spring_same_res,
                    EdgeKind::HBond | EdgeKind::SaltBridge => p.k_spring_intra_polar,
                    EdgeKind::Clash | EdgeKind::VanDerWaalsContact => p.k_spring_first_shell,
                };
                let f = k * (dist - edge.dist_3d_a) * (delta / dist);
                forces[i] += f;
                forces[j] -= f;
            } else if dist > p.cross_reg_deadzone_a {
                // No dedicated stiff cross-link constant exists yet; a cross-interface disulfide
                // or other covalent link is bucketed with the existing apolar regularization for
                // now (a simplification, not a studied choice — worth a dedicated constant later).
                let k_reg = match edge.kind {
                    EdgeKind::HBond | EdgeKind::SaltBridge | EdgeKind::Clash => p.k_reg_cross_polar,
                    EdgeKind::VanDerWaalsContact | EdgeKind::Covalent(_) => p.k_reg_cross_apolar,
                };
                let f = k_reg * (dist - p.cross_reg_target_a) * (delta / dist);
                forces[i] += f;
                forces[j] -= f;
            }
        }

        // 2. Same-side hard-core non-overlap repulsion
        for i in 0..n {
            for j in (i + 1)..n {
                if nodes[i].side != nodes[j].side {
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
            if terr.member_atoms.len() <= 1 {
                continue;
            }
            let centroid = terr
                .member_atoms
                .iter()
                .map(|&idx| pos[idx.0 as usize])
                .sum::<Vec2>()
                / (terr.member_atoms.len() as f32);
            for &idx in &terr.member_atoms {
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
            na.pos_3d, na.pos_uv, na.side, plane, flatten_t, 0.0, 0.0,
        ));
        let pb = Vec3::from(interpolate_atom_position(
            nb.pos_3d, nb.pos_uv, nb.side, plane, flatten_t, 0.0, 0.0,
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
            if nodes[i].side != nodes[j].side || nodes[i].territory == nodes[j].territory {
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
    hinge_u: f32,
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
            let du = u - hinge_u;
            let u_rot = hinge_u + du * phi.cos();
            let n_rot = -du * phi.sin() - sheet_offset * phi.cos();
            origin + u_rot * axis_u + v * axis_v + n_rot * normal
        }
        InterfaceSide::None => p3,
    };

    let t = flatten_t.clamp(0.0, 1.0);
    p3.lerp(p_flat, t).to_array()
}
