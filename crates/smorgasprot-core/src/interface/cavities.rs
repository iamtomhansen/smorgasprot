//! Probe-accessible interfacial cavity detection, solvent-connectivity topology, and meshing.
//!
//! Uses morphological opening (sphere erosion at `probe_radius_a` followed by probe-sphere
//! dilation) on the interface corridor, classifying each probe-accessible component as either
//! a sealed buried cavity or a solvent-connected channel.

use super::definition::InterfaceSide;
use super::geometry::{GeometryParams, extract_dual_surface_net};
use super::model::AtomNode;
use crate::model::{ComplexStructure, GlobalAtomIdx, SurfaceMesh};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

crate::param_group! {
    /// Probe-accessible interfacial cavity detection parameters.
    pub struct CavityParams("cavities", "Probe Cavities") {
        /// Probe sphere radius (Å), e.g. 1.4 Å for water or 0.5–1.0 Å for proton/ion probes.
        probe_radius_a: f32 = 1.4, 0.25..=3.0, "Probe radius", "Å";
        /// Voxel grid spacing for cavity erosion/dilation (Å); finer resolves smaller probes.
        grid_step_a: f32 = 0.65, 0.25..=2.0, "Cavity grid step", "Å";
        /// Minimum cavity component volume to retain (Å³), filtering sub-voxel noise.
        min_volume_a3: f32 = 1.0, 0.0..=50.0, "Min cavity volume", "Å³";
    }
}

/// Probe-accessible interfacial cavities partitioned into buried pockets and solvent channels.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CavityAnalysis {
    /// Sealed probe-accessible pockets with no collision-free path of `probe_radius_a` to bulk solvent.
    pub buried_mesh: SurfaceMesh,
    /// Probe-accessible channels connected to bulk solvent by a collision-free path of `probe_radius_a`.
    pub channel_mesh: SurfaceMesh,
    /// Total enclosed volume of buried cavities (Å³).
    pub buried_volume_a3: f32,
    /// Total enclosed volume of solvent-connected channels (Å³).
    pub channel_volume_a3: f32,
    /// Combined probe-accessible cavity volume (`buried_volume_a3 + channel_volume_a3`) (Å³).
    pub total_volume_a3: f32,
    /// Number of distinct connected buried cavity components.
    pub buried_count: usize,
    /// Number of distinct connected solvent-channel components.
    pub channel_count: usize,
}

/// 6-connected BFS over a 3D grid from all `exterior_seed && passable` cells.
/// Returns a mask of every `passable` cell reachable from the exterior.
pub fn flood_fill_solvent_connectivity(
    passable: &[bool],
    exterior_seed: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<bool> {
    let total = nx * ny * nz;
    let mut reachable = vec![false; total];
    let mut queue = VecDeque::new();
    let stride_y = nz;
    let stride_x = ny * nz;

    for g in 0..total {
        if passable[g] && exterior_seed[g] {
            reachable[g] = true;
            queue.push_back(g);
        }
    }

    while let Some(g) = queue.pop_front() {
        let ix = g / stride_x;
        let rem = g % stride_x;
        let iy = rem / stride_y;
        let iz = rem % stride_y;

        let mut try_push = |nx_i: usize, ny_i: usize, nz_i: usize| {
            let ng = nx_i * stride_x + ny_i * stride_y + nz_i;
            if passable[ng] && !reachable[ng] {
                reachable[ng] = true;
                queue.push_back(ng);
            }
        };

        if ix > 0 {
            try_push(ix - 1, iy, iz);
        }
        if ix + 1 < nx {
            try_push(ix + 1, iy, iz);
        }
        if iy > 0 {
            try_push(ix, iy - 1, iz);
        }
        if iy + 1 < ny {
            try_push(ix, iy + 1, iz);
        }
        if iz > 0 {
            try_push(ix, iy, iz - 1);
        }
        if iz + 1 < nz {
            try_push(ix, iy, iz + 1);
        }
    }

    reachable
}

/// Partitions `mesh.triangles` into connected 2-manifold components via shared vertices.
pub fn mesh_connected_components(mesh: &SurfaceMesh) -> Vec<Vec<usize>> {
    let n_verts = mesh.vertices.len();
    let n_tris = mesh.triangles.len();
    if n_verts == 0 || n_tris == 0 {
        return Vec::new();
    }

    let mut vert_to_tris: Vec<Vec<usize>> = vec![Vec::new(); n_verts];
    for (t_idx, &[a, b, c]) in mesh.triangles.iter().enumerate() {
        for v in [a as usize, b as usize, c as usize] {
            if v < n_verts {
                vert_to_tris[v].push(t_idx);
            }
        }
    }

    let mut visited = vec![false; n_tris];
    let mut components = Vec::new();
    let mut queue = VecDeque::new();

    for start in 0..n_tris {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        queue.push_back(start);
        let mut comp = Vec::new();

        while let Some(t) = queue.pop_front() {
            comp.push(t);
            let [a, b, c] = mesh.triangles[t];
            for v in [a as usize, b as usize, c as usize] {
                if v < n_verts {
                    for &nbr in &vert_to_tris[v] {
                        if !visited[nbr] {
                            visited[nbr] = true;
                            queue.push_back(nbr);
                        }
                    }
                }
            }
        }
        components.push(comp);
    }

    components
}

/// Filters a triangle mesh to connected components with enclosed volume >= `min_volume_a3`.
/// Returns `(compacted_mesh, kept_component_count, total_volume_a3)`.
pub fn filter_mesh_components_by_volume(mesh: &SurfaceMesh, min_volume_a3: f32) -> (SurfaceMesh, usize, f32) {
    let components = mesh_connected_components(mesh);
    if components.is_empty() {
        return (SurfaceMesh::default(), 0, 0.0);
    }

    let mut kept_tris = Vec::new();
    let mut kept_count = 0usize;
    let mut total_vol = 0.0_f32;

    for comp in components {
        let mut signed_vol_6 = 0.0_f64;
        for &t_idx in &comp {
            let [i0, i1, i2] = mesh.triangles[t_idx];
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
            if u0 < mesh.vertices.len() && u1 < mesh.vertices.len() && u2 < mesh.vertices.len() {
                let v0 = Vec3::from(mesh.vertices[u0]);
                let v1 = Vec3::from(mesh.vertices[u1]);
                let v2 = Vec3::from(mesh.vertices[u2]);
                signed_vol_6 += v0.dot(v1.cross(v2)) as f64;
            }
        }
        let vol = (signed_vol_6 / 6.0).abs() as f32;
        if vol >= min_volume_a3 {
            kept_count += 1;
            total_vol += vol;
            for &t_idx in &comp {
                kept_tris.push(mesh.triangles[t_idx]);
            }
        }
    }

    if kept_tris.is_empty() {
        return (SurfaceMesh::default(), 0, 0.0);
    }

    // Compact vertex buffer to only vertices referenced by kept triangles.
    let mut remap = vec![u32::MAX; mesh.vertices.len()];
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut nearest_atom = Vec::new();
    let mut triangles = Vec::with_capacity(kept_tris.len());

    for [i0, i1, i2] in kept_tris {
        let mut out = [0u32; 3];
        for (slot, old_idx) in [i0, i1, i2].into_iter().enumerate() {
            let u = old_idx as usize;
            if remap[u] == u32::MAX {
                let new_idx = vertices.len() as u32;
                remap[u] = new_idx;
                vertices.push(mesh.vertices[u]);
                normals.push(mesh.normals.get(u).copied().unwrap_or([0.0, 1.0, 0.0]));
                nearest_atom.push(mesh.nearest_atom.get(u).copied().unwrap_or(GlobalAtomIdx(0)));
            }
            out[slot] = remap[u];
        }
        triangles.push(out);
    }

    (
        SurfaceMesh {
            vertices,
            normals,
            nearest_atom,
            triangles,
        },
        kept_count,
        total_vol,
    )
}

/// Evaluates the dilated probe-sphere union SDF `psi(q) = min_c (||q - c|| - probe_radius)`.
fn dilate_probe_centers(
    centers: &[(usize, usize, usize)],
    nx: usize,
    ny: usize,
    nz: usize,
    step: f32,
    probe_radius: f32,
) -> Vec<f32> {
    let total = nx * ny * nz;
    let mut psi = vec![10.0_f32; total];
    if centers.is_empty() {
        return psi;
    }

    let stride_y = nz;
    let stride_x = ny * nz;
    let reach_vox = ((probe_radius + step * 1.5) / step).ceil() as isize;

    for &(cx, cy, cz) in centers {
        let x0 = (cx as isize - reach_vox).max(0) as usize;
        let x1 = (cx as isize + reach_vox).min(nx as isize - 1) as usize;
        let y0 = (cy as isize - reach_vox).max(0) as usize;
        let y1 = (cy as isize + reach_vox).min(ny as isize - 1) as usize;
        let z0 = (cz as isize - reach_vox).max(0) as usize;
        let z1 = (cz as isize + reach_vox).min(nz as isize - 1) as usize;

        for ix in x0..=x1 {
            let dx = (ix as f32 - cx as f32) * step;
            let dx2 = dx * dx;
            for iy in y0..=y1 {
                let dy = (iy as f32 - cy as f32) * step;
                let dxy2 = dx2 + dy * dy;
                for iz in z0..=z1 {
                    let dz = (iz as f32 - cz as f32) * step;
                    let d = (dxy2 + dz * dz).sqrt() - probe_radius;
                    let g = ix * stride_x + iy * stride_y + iz;
                    if d < psi[g] {
                        psi[g] = d;
                    }
                }
            }
        }
    }

    psi
}

/// Computes probe-accessible buried cavities and solvent-connected channels in the interface.
pub fn build_interface_cavities(
    complex: &ComplexStructure,
    nodes: &[AtomNode],
    geom_p: &GeometryParams,
    p: &CavityParams,
) -> CavityAnalysis {
    if nodes.is_empty() {
        return CavityAnalysis::default();
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
        let r = atom.vdw_radius_a + geom_p.implicit_h_skin_a;
        min_pt = min_pt.min(pos);
        max_pt = max_pt.max(pos);

        match node.side {
            InterfaceSide::SideA => side_a_atoms.push((pos, r, node.global_atom)),
            InterfaceSide::SideB => side_b_atoms.push((pos, r, node.global_atom)),
            InterfaceSide::None => {}
        }
    }

    if side_a_atoms.is_empty() || side_b_atoms.is_empty() {
        return CavityAnalysis::default();
    }

    let margin = geom_p.void_bounding_margin_a.max(p.probe_radius_a + p.grid_step_a * 2.0);
    min_pt -= Vec3::splat(margin);
    max_pt += Vec3::splat(margin);

    let step = p.grid_step_a.max(0.2);
    let inv_step = 1.0 / step;
    let nx = (((max_pt.x - min_pt.x) * inv_step).ceil() as usize + 1).max(4);
    let ny = (((max_pt.y - min_pt.y) * inv_step).ceil() as usize + 1).max(4);
    let nz = (((max_pt.z - min_pt.z) * inv_step).ceil() as usize + 1).max(4);
    let stride_y = nz;
    let stride_x = ny * nz;
    let total_pts = nx * ny * nz;

    let mut phi_a = vec![f32::INFINITY; total_pts];
    let mut phi_b = vec![f32::INFINITY; total_pts];
    let mut nearest_atom_grid = vec![nodes[0].global_atom; total_pts];
    let mut best_dist_grid = vec![f32::INFINITY; total_pts];

    let mut splat_atoms = |atoms: &[(Vec3, f32, GlobalAtomIdx)], phi: &mut [f32]| {
        for &(center, r_i, g_atom) in atoms {
            let reach = r_i + geom_p.void_max_gap_a + p.probe_radius_a + step * 2.0;
            let ix_min = (((center.x - reach - min_pt.x) * inv_step).floor() as isize).max(0) as usize;
            let ix_max = (((center.x + reach - min_pt.x) * inv_step).ceil() as usize).min(nx.saturating_sub(1));
            let iy_min = (((center.y - reach - min_pt.y) * inv_step).floor() as isize).max(0) as usize;
            let iy_max = (((center.y + reach - min_pt.y) * inv_step).ceil() as usize).min(ny.saturating_sub(1));
            let iz_min = (((center.z - reach - min_pt.z) * inv_step).floor() as isize).max(0) as usize;
            let iz_max = (((center.z + reach - min_pt.z) * inv_step).ceil() as usize).min(nz.saturating_sub(1));

            for ix in ix_min..=ix_max {
                let px = min_pt.x + (ix as f32) * step;
                let dx = px - center.x;
                for iy in iy_min..=iy_max {
                    let py = min_pt.y + (iy as f32) * step;
                    let dy = py - center.y;
                    let dxy2 = dx * dx + dy * dy;
                    for iz in iz_min..=iz_max {
                        let pz = min_pt.z + (iz as f32) * step;
                        let dz = pz - center.z;
                        let d_surf = (dxy2 + dz * dz).sqrt() - r_i;
                        let g_idx = ix * stride_x + iy * stride_y + iz;
                        if d_surf < phi[g_idx] {
                            phi[g_idx] = d_surf;
                        }
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

    // Erode empty space by probe_radius_a and classify solvent connectivity.
    let mut passable = vec![false; total_pts];
    let mut in_corridor = vec![false; total_pts];
    let mut exterior_seed = vec![false; total_pts];

    for ix in 0..nx {
        let on_x_edge = ix == 0 || ix + 1 == nx;
        for iy in 0..ny {
            let on_y_edge = iy == 0 || iy + 1 == ny;
            for iz in 0..nz {
                let on_z_edge = iz == 0 || iz + 1 == nz;
                let g = ix * stride_x + iy * stride_y + iz;
                let d_complex = phi_a[g].min(phi_b[g]);
                let d_max = phi_a[g].max(phi_b[g]);
                let corridor = !(on_x_edge || on_y_edge || on_z_edge) && d_max <= geom_p.void_max_gap_a;
                in_corridor[g] = corridor;
                if d_complex >= p.probe_radius_a {
                    passable[g] = true;
                    if !corridor {
                        exterior_seed[g] = true;
                    }
                }
            }
        }
    }

    let solvent_reachable = flood_fill_solvent_connectivity(&passable, &exterior_seed, nx, ny, nz);

    let mut buried_centers = Vec::new();
    let mut channel_centers = Vec::new();
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let g = ix * stride_x + iy * stride_y + iz;
                if passable[g] && in_corridor[g] {
                    if solvent_reachable[g] {
                        channel_centers.push((ix, iy, iz));
                    } else {
                        buried_centers.push((ix, iy, iz));
                    }
                }
            }
        }
    }

    let psi_buried = dilate_probe_centers(&buried_centers, nx, ny, nz, step, p.probe_radius_a);
    let psi_channel = dilate_probe_centers(&channel_centers, nx, ny, nz, step, p.probe_radius_a);

    let fallback = nodes[0].global_atom;
    let buried_raw = extract_dual_surface_net(
        &psi_buried,
        nx,
        ny,
        nz,
        min_pt,
        step,
        &nearest_atom_grid,
        &best_dist_grid,
        fallback,
    );
    let channel_raw = extract_dual_surface_net(
        &psi_channel,
        nx,
        ny,
        nz,
        min_pt,
        step,
        &nearest_atom_grid,
        &best_dist_grid,
        fallback,
    );

    let (buried_mesh, buried_count, buried_volume_a3) =
        filter_mesh_components_by_volume(&buried_raw, p.min_volume_a3);
    let (channel_mesh, channel_count, channel_volume_a3) =
        filter_mesh_components_by_volume(&channel_raw, p.min_volume_a3);

    CavityAnalysis {
        buried_mesh,
        channel_mesh,
        buried_volume_a3,
        channel_volume_a3,
        total_volume_a3: buried_volume_a3 + channel_volume_a3,
        buried_count,
        channel_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::model::{AtomNodeIdx, TerritoryIdx};
    use crate::model::{AtomRecord, Element, GlobalChainIdx, GlobalResidueIdx};

    fn make_complex_and_nodes(atoms: &[([f32; 3], f32, InterfaceSide)]) -> (ComplexStructure, Vec<AtomNode>) {
        let complex = ComplexStructure {
            name: "cavity_test".into(),
            chains: Vec::new(),
            residues: Vec::new(),
            atoms: atoms
                .iter()
                .enumerate()
                .map(|(i, &(pos_3d, vdw_radius_a, _))| AtomRecord {
                    idx: GlobalAtomIdx(i as u32),
                    residue_idx: GlobalResidueIdx(0),
                    chain_idx: GlobalChainIdx(0),
                    atom_serial: i + 1,
                    atom_name: *b"CA  ",
                    element: Element::C,
                    vdw_radius_a,
                    pos_3d,
                    b_factor: 0.0,
                    occupancy: 1.0,
                })
                .collect(),
            bonds: Vec::new(),
        };
        let nodes = atoms
            .iter()
            .enumerate()
            .map(|(i, &(pos_3d, _, side))| AtomNode {
                idx: AtomNodeIdx(i as u32),
                global_atom: GlobalAtomIdx(i as u32),
                territory: TerritoryIdx(0),
                side,
                b_factor: 0.0,
                free_sasa_a2: 0.0,
                complex_sasa_a2: 0.0,
                buried_sasa_a2: 0.0,
                aliphatic_score: 0.0,
                pos_3d,
                pos_uv_raw: [pos_3d[0], pos_3d[1]],
                pos_uv: [pos_3d[0], pos_3d[1]],
                is_unsatisfied_buried_polar: false,
                interactions: Vec::new(),
            })
            .collect();
        (complex, nodes)
    }

    #[test]
    fn empty_or_one_sided_returns_empty() {
        let (c, n) = make_complex_and_nodes(&[([0.0, 0.0, 0.0], 1.7, InterfaceSide::SideA)]);
        let res = build_interface_cavities(&c, &n, &GeometryParams::default(), &CavityParams::default());
        assert_eq!(res.buried_count, 0);
        assert_eq!(res.channel_count, 0);
        assert_eq!(res.total_volume_a3, 0.0);
    }

    #[test]
    fn sealed_pocket_yields_buried_cavity_and_large_probe_excludes_it() {
        // Two flat slabs at z = -3.0 (SideA) and z = +3.0 (SideB), sealed around the perimeter
        // at radius 4.5 Å, leaving a central pocket of ~1.3 Å clearance.
        let mut atoms = Vec::new();
        for ix in -3..=3 {
            for iy in -3..=3 {
                let (x, y) = (ix as f32 * 1.5, iy as f32 * 1.5);
                let r2 = x * x + y * y;
                if r2 > 6.0 {
                    // Perimeter wall bridging SideA and SideB
                    atoms.push(([x, y, 0.0], 2.2, InterfaceSide::SideA));
                    atoms.push(([x, y, 1.0], 2.2, InterfaceSide::SideB));
                } else {
                    atoms.push(([x, y, -3.2], 1.7, InterfaceSide::SideA));
                    atoms.push(([x, y, 3.2], 1.7, InterfaceSide::SideB));
                }
            }
        }
        let (c, n) = make_complex_and_nodes(&atoms);
        let geom = GeometryParams {
            void_max_gap_a: 6.0,
            implicit_h_skin_a: 0.0,
            ..GeometryParams::default()
        };

        let small_probe = CavityParams {
            probe_radius_a: 0.8,
            grid_step_a: 0.5,
            min_volume_a3: 1.0,
        };
        let res_small = build_interface_cavities(&c, &n, &geom, &small_probe);
        assert!(res_small.buried_count >= 1, "expected buried cavity for 0.8 Å probe");
        assert!(res_small.buried_volume_a3 > 1.0);

        let huge_probe = CavityParams {
            probe_radius_a: 2.5,
            grid_step_a: 0.5,
            min_volume_a3: 1.0,
        };
        let res_huge = build_interface_cavities(&c, &n, &geom, &huge_probe);
        assert_eq!(res_huge.buried_count, 0, "2.5 Å probe should not fit in ~1.5 Å half-gap");
    }

    #[test]
    fn open_corridor_yields_solvent_channel() {
        // Two parallel slabs at z = -3.0 (SideA) and z = +3.0 (SideB) with no perimeter seal.
        let mut atoms = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                let (x, y) = (ix as f32 * 2.0, iy as f32 * 2.0);
                atoms.push(([x, y, -3.0], 1.7, InterfaceSide::SideA));
                atoms.push(([x, y, 3.0], 1.7, InterfaceSide::SideB));
            }
        }
        let (c, n) = make_complex_and_nodes(&atoms);
        let geom = GeometryParams {
            void_max_gap_a: 5.5,
            implicit_h_skin_a: 0.0,
            ..GeometryParams::default()
        };
        let p = CavityParams {
            probe_radius_a: 0.8,
            grid_step_a: 0.5,
            min_volume_a3: 1.0,
        };
        let res = build_interface_cavities(&c, &n, &geom, &p);
        assert!(res.channel_count >= 1, "open corridor should connect to solvent");
        assert!(res.channel_volume_a3 > 1.0);
        assert_eq!(res.buried_count, 0);
    }
}
