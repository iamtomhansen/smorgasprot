//! Probes for cavities of open space using spherical probes of arbitrary radius.
//! 
//! ## Summary
//! Identifies probe-accessible cavities in the interfacial space. Useful to detect buried pockets
//! and solvent-accessible channels. Probe sizing can be calibrated to e.g. water (1.4 Å), proton
//! (1.2 Å), or ion (Ca2+ 1.0 Å) radius.
//!
//! ## Considerations
//! To find all available cavities, a theoretically infinitely small grid would be required. In
//! practice, a cavity of exactly 1.4 Å is likely too small for a solvent molecule to actually
//! settle, so tuning the grid step and probe radius is more informative than striving for infinite
//! compute power. What we are doing here is apparently called "grid-based morphological opening",
//! which is different from the rolling marble approach in e.g. Rosetta or CAVER. To improve
//! results, iterative refinement of detected cavities by subdividing the grid is enabled (prevents
//! voxel "aliasing", where the cavities match the shape of the search grid).
//! 
//! Once a cavity is detected, its mesh can be dilated to find the largest probe that can fit
//! inside, as well as the smallest-probe extents. It an also be determined whether that cavity is
//! sealed by the interface or connected to bulk solvent, as well as the size of the largest probe
//! that can pass through the channel to bulk solvent.
//! 
//! ## References
//! -  Volkov, A.G., Paula, S., Deamer, D.W. (1997). Two mechanisms of permeation of small neutral
//!    molecules and ions across phospholipid bilayers. https://doi.org/10.1016/S0302-4598(96)05097-0
//!    (for probe radius)
//! 
//! ## TODO
//! - Add calculations for largest probe that can pass through a channel to bulk solvent (currently only largest probe that can fit inside the cavity is computed).
//! - Do something about the ring of voxels around the interface that are not so much cavities as "external crevices".

use super::definition::InterfaceSide;
use super::geometry::{GeometryParams, extract_dual_surface_net};
use super::model::AtomNode;
use crate::model::{ComplexStructure, GlobalAtomIdx, SurfaceMesh};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

crate::param_group! {
    /// Probe-accessible interfacial cavity detection parameters.
    pub struct CavityParams("cavities", "Probe Cavities") {
        /// Probe sphere radius (Å), e.g. 1.4 Å for water or 0.5–1.0 Å for proton/ion probes.
        #[desc = "The radius of the probe used to check for cavities. Water is ~1.4 Å."]
        probe_radius_a: f32 = 1.4, 0.25..=3.0, "Probe radius", "Å";
        /// Voxel grid spacing for cavity discovery (Å); finer resolves smaller probes.
        #[desc = "The spacing of the coarse voxel grid used for cavity discovery. Finer grids detect more cavities but take more time to compute."]
        grid_step_a: f32 = 0.65, 0.25..=2.0, "Cavity grid step", "Å";
        /// Number of local 2× grid subdivisions around discovered cavities (0 = coarse only, 4 = 1/16 step).
        #[desc = "Local 2x grid refinement iterations around discovered cavities to eliminate voxel aliasing."]
        refinement_iterations: usize = 1, 0..=4, "Local refinement iterations", "";
        /// Minimum cavity component volume to retain (Å³), filtering small cavities.
        #[desc = "The minimum volume of a cavity to be reported. Smaller cavities are ignored."]
        min_volume_a3: f32 = 1.0, 0.0..=50.0, "Min cavity volume", "Å³";
    }
}

/// Per-cavity geometric summary and discretization error bound.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CavityRecord {
    /// Dense index of this cavity in the `CavityAnalysis.cavities` vector.
    pub id: usize,
    /// True if this cavity has no channel to bulk solvent.
    pub is_buried: bool,
    /// Total enclosed volume of this cavity (Å³).
    pub volume_a3: f32,
    /// Worst-case volume discretization error (`surface_area_a2 * worst_case_radial_error_a`) (Å³).
    pub worst_case_volume_error_a3: f32,
    /// Largest collision-free sphere radius (Å) that fits inside this cavity.
    pub max_internal_radius_a: f32,
    /// Largest probe radius (Å) that can pass from bulk solvent to this cavity (`>= probe_radius_a`
    /// for channels, `< probe_radius_a` for buried pockets).
    pub bottleneck_radius_a: f32,
    /// Centroid of this cavity (Å) in global coordinates.
    pub centroid: [f32; 3],
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
    /// Effective voxel spacing (Å) after `refinement_iterations` halvings (`grid_step_a / 2^k`).
    pub effective_step_a: f32,
    /// Worst-case radial/positional discretization error (`0.5 * sqrt(3) * effective_step_a`) (Å).
    pub worst_case_radial_error_a: f32,
    /// Per-cavity breakdown (volume, error bound, max internal radius, centroid).
    pub cavities: Vec<CavityRecord>,
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

    splat_atoms_onto_grid(
        &side_a_atoms,
        min_pt,
        nx,
        ny,
        nz,
        step,
        geom_p.void_max_gap_a + p.probe_radius_a + step * 2.0,
        &mut phi_a,
        &mut nearest_atom_grid,
        &mut best_dist_grid,
    );
    splat_atoms_onto_grid(
        &side_b_atoms,
        min_pt,
        nx,
        ny,
        nz,
        step,
        geom_p.void_max_gap_a + p.probe_radius_a + step * 2.0,
        &mut phi_b,
        &mut nearest_atom_grid,
        &mut best_dist_grid,
    );

    // 2. Erode empty space by probe_radius_a and classify solvent connectivity.
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
    let bottleneck = compute_bottleneck_radii(&phi_a, &phi_b, &exterior_seed, nx, ny, nz);

    // 3. Partition coarse passable corridor centers into buried vs. channel 6-connected components.
    let mut is_buried_center = vec![false; total_pts];
    let mut is_channel_center = vec![false; total_pts];
    for g in 0..total_pts {
        if passable[g] && in_corridor[g] {
            if solvent_reachable[g] {
                is_channel_center[g] = true;
            } else {
                is_buried_center[g] = true;
            }
        }
    }

    let buried_components = group_center_components(&is_buried_center, nx, ny, nz);
    let channel_components = group_center_components(&is_channel_center, nx, ny, nz);

    // 4. Compute effective step and worst-case radial error after local 2^k subdivision.
    let refine_k = p.refinement_iterations.min(4) as u32;
    let effective_step_a = step / (1u32 << refine_k) as f32;
    let worst_case_radial_error_a = 0.5 * 3.0_f32.sqrt() * effective_step_a;

    let fallback = nodes[0].global_atom;
    let mut buried_mesh = SurfaceMesh::default();
    let mut channel_mesh = SurfaceMesh::default();
    let mut buried_volume_a3 = 0.0_f32;
    let mut channel_volume_a3 = 0.0_f32;
    let mut buried_count = 0usize;
    let mut channel_count = 0usize;
    let mut cavities = Vec::new();

    // 5. Refine each discovered cavity locally (if refinement_iterations > 0) and build per-cavity stats.
    let mut process_components = |components: Vec<Vec<(usize, usize, usize)>>, is_buried: bool| {
        for comp in components {
            let bottleneck_radius_a = comp
                .iter()
                .map(|&(ix, iy, iz)| bottleneck[ix * stride_x + iy * stride_y + iz])
                .fold(0.0_f32, f32::max);

            let (comp_mesh, max_internal_radius_a) = if refine_k == 0 {
                let mut max_r = 0.0_f32;
                for &(ix, iy, iz) in &comp {
                    let g = ix * stride_x + iy * stride_y + iz;
                    max_r = max_r.max(phi_a[g].min(phi_b[g]));
                }
                let psi = dilate_probe_centers(&comp, nx, ny, nz, step, p.probe_radius_a);
                let mesh = extract_dual_surface_net(
                    &psi,
                    nx,
                    ny,
                    nz,
                    min_pt,
                    step,
                    &nearest_atom_grid,
                    &best_dist_grid,
                    fallback,
                );
                (mesh, max_r)
            } else {
                refine_and_mesh_component(
                    &comp,
                    min_pt,
                    step,
                    effective_step_a,
                    &side_a_atoms,
                    &side_b_atoms,
                    geom_p.void_max_gap_a,
                    p.probe_radius_a,
                    fallback,
                )
            };

            let (volume_a3, surface_area_a2, centroid) = mesh_volume_area_centroid(&comp_mesh);
            if volume_a3 < p.min_volume_a3 {
                continue;
            }

            let id = cavities.len();
            cavities.push(CavityRecord {
                id,
                is_buried,
                volume_a3,
                worst_case_volume_error_a3: surface_area_a2 * worst_case_radial_error_a,
                max_internal_radius_a,
                bottleneck_radius_a,
                centroid,
            });

            if is_buried {
                buried_count += 1;
                buried_volume_a3 += volume_a3;
                append_mesh(&mut buried_mesh, &comp_mesh);
            } else {
                channel_count += 1;
                channel_volume_a3 += volume_a3;
                append_mesh(&mut channel_mesh, &comp_mesh);
            }
        }
    };

    process_components(buried_components, true);
    process_components(channel_components, false);

    CavityAnalysis {
        buried_mesh,
        channel_mesh,
        buried_volume_a3,
        channel_volume_a3,
        total_volume_a3: buried_volume_a3 + channel_volume_a3,
        buried_count,
        channel_count,
        effective_step_a,
        worst_case_radial_error_a,
        cavities,
    }
}

/// Splat atoms onto a 3D grid, updating the SDF `phi`, nearest atom indices, and best distances.
#[allow(clippy::too_many_arguments)]
fn splat_atoms_onto_grid(
    atoms: &[(Vec3, f32, GlobalAtomIdx)],
    min_pt: Vec3,
    nx: usize,
    ny: usize,
    nz: usize,
    step: f32,
    extra_reach_a: f32,
    phi: &mut [f32],
    nearest_atom_grid: &mut [GlobalAtomIdx],
    best_dist_grid: &mut [f32],
) {
    let inv_step = 1.0 / step;
    let stride_y = nz;
    let stride_x = ny * nz;

    for &(center, r_i, g_atom) in atoms {
        let reach = r_i + extra_reach_a;
        let ix_min = (((center.x - reach - min_pt.x) * inv_step).floor() as isize).max(0) as usize;
        let ix_max =
            (((center.x + reach - min_pt.x) * inv_step).ceil() as usize).min(nx.saturating_sub(1));
        let iy_min = (((center.y - reach - min_pt.y) * inv_step).floor() as isize).max(0) as usize;
        let iy_max =
            (((center.y + reach - min_pt.y) * inv_step).ceil() as usize).min(ny.saturating_sub(1));
        let iz_min = (((center.z - reach - min_pt.z) * inv_step).floor() as isize).max(0) as usize;
        let iz_max =
            (((center.z + reach - min_pt.z) * inv_step).ceil() as usize).min(nz.saturating_sub(1));

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
}

/// Groups true voxels in `mask` into 6-connected components of `(ix, iy, iz)` coordinates.
fn group_center_components(
    mask: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<Vec<(usize, usize, usize)>> {
    let total = nx * ny * nz;
    let stride_y = nz;
    let stride_x = ny * nz;
    let mut visited = vec![false; total];
    let mut components = Vec::new();
    let mut queue = VecDeque::new();

    for start in 0..total {
        if !mask[start] || visited[start] {
            continue;
        }
        visited[start] = true;
        queue.push_back(start);
        let mut comp = Vec::new();

        while let Some(g) = queue.pop_front() {
            let ix = g / stride_x;
            let rem = g % stride_x;
            let iy = rem / stride_y;
            let iz = rem % stride_y;
            comp.push((ix, iy, iz));

            let mut try_push = |nx_i: usize, ny_i: usize, nz_i: usize| {
                let ng = nx_i * stride_x + ny_i * stride_y + nz_i;
                if mask[ng] && !visited[ng] {
                    visited[ng] = true;
                    queue.push_back(ng);
                }
            };
            if ix > 0 { try_push(ix - 1, iy, iz); }
            if ix + 1 < nx { try_push(ix + 1, iy, iz); }
            if iy > 0 { try_push(ix, iy - 1, iz); }
            if iy + 1 < ny { try_push(ix, iy + 1, iz); }
            if iz > 0 { try_push(ix, iy, iz - 1); }
            if iz + 1 < nz { try_push(ix, iy, iz + 1); }
        }
        components.push(comp);
    }

    components
}

/// Refines a single coarse cavity component on a local sub-grid of spacing `eff_step`.
#[allow(clippy::too_many_arguments)]
fn refine_and_mesh_component(
    coarse_centers: &[(usize, usize, usize)],
    coarse_min_pt: Vec3,
    coarse_step: f32,
    eff_step: f32,
    side_a_atoms: &[(Vec3, f32, GlobalAtomIdx)],
    side_b_atoms: &[(Vec3, f32, GlobalAtomIdx)],
    void_max_gap_a: f32,
    probe_radius_a: f32,
    fallback: GlobalAtomIdx,
) -> (SurfaceMesh, f32) {
    let mut c_min = Vec3::splat(f32::INFINITY);
    let mut c_max = Vec3::splat(f32::NEG_INFINITY);
    for &(ix, iy, iz) in coarse_centers {
        let p = coarse_min_pt + Vec3::new(ix as f32, iy as f32, iz as f32) * coarse_step;
        c_min = c_min.min(p);
        c_max = c_max.max(p);
    }

    let pad = probe_radius_a + coarse_step * 2.0;
    let local_min = c_min - Vec3::splat(pad);
    let local_max = c_max + Vec3::splat(pad);

    // Cap local axis resolution to 96 voxels to bound memory on large channels at k = 4.
    let span = local_max - local_min;
    let max_span = span.x.max(span.y).max(span.z);
    let local_step = eff_step.max(max_span / 92.0);
    let inv_step = 1.0 / local_step;

    let nx = ((span.x * inv_step).ceil() as usize + 1).max(4);
    let ny = ((span.y * inv_step).ceil() as usize + 1).max(4);
    let nz = ((span.z * inv_step).ceil() as usize + 1).max(4);
    let total = nx * ny * nz;
    let stride_y = nz;
    let stride_x = ny * nz;

    let mut phi_a = vec![f32::INFINITY; total];
    let mut phi_b = vec![f32::INFINITY; total];
    let mut nearest_atom_grid = vec![fallback; total];
    let mut best_dist_grid = vec![f32::INFINITY; total];

    let reach = void_max_gap_a + probe_radius_a + local_step * 2.0;
    splat_atoms_onto_grid(
        side_a_atoms,
        local_min,
        nx,
        ny,
        nz,
        local_step,
        reach,
        &mut phi_a,
        &mut nearest_atom_grid,
        &mut best_dist_grid,
    );
    splat_atoms_onto_grid(
        side_b_atoms,
        local_min,
        nx,
        ny,
        nz,
        local_step,
        reach,
        &mut phi_b,
        &mut nearest_atom_grid,
        &mut best_dist_grid,
    );

    let mut passable = vec![false; total];
    for g in 0..total {
        let d_complex = phi_a[g].min(phi_b[g]);
        let d_max = phi_a[g].max(phi_b[g]);
        if d_complex >= probe_radius_a && d_max <= void_max_gap_a {
            passable[g] = true;
        }
    }

    let mut seed = vec![false; total];
    for &(cx, cy, cz) in coarse_centers {
        let p = coarse_min_pt + Vec3::new(cx as f32, cy as f32, cz as f32) * coarse_step;
        let lx = (((p.x - local_min.x) * inv_step).round() as isize).clamp(0, nx as isize - 1) as usize;
        let ly = (((p.y - local_min.y) * inv_step).round() as isize).clamp(0, ny as isize - 1) as usize;
        let lz = (((p.z - local_min.z) * inv_step).round() as isize).clamp(0, nz as isize - 1) as usize;
        seed[lx * stride_x + ly * stride_y + lz] = true;
    }

    let reachable = flood_fill_solvent_connectivity(&passable, &seed, nx, ny, nz);
    let mut refined_centers = Vec::new();
    let mut max_internal_radius_a = 0.0_f32;
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let g = ix * stride_x + iy * stride_y + iz;
                if reachable[g] {
                    refined_centers.push((ix, iy, iz));
                    max_internal_radius_a = max_internal_radius_a.max(phi_a[g].min(phi_b[g]));
                }
            }
        }
    }

    let psi = dilate_probe_centers(&refined_centers, nx, ny, nz, local_step, probe_radius_a);
    let mesh = extract_dual_surface_net(
        &psi,
        nx,
        ny,
        nz,
        local_min,
        local_step,
        &nearest_atom_grid,
        &best_dist_grid,
        fallback,
    );
    (mesh, max_internal_radius_a)
}

/// Computes `(volume_a3, surface_area_a2, centroid)` for a closed triangle mesh.
fn mesh_volume_area_centroid(mesh: &SurfaceMesh) -> (f32, f32, [f32; 3]) {
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

/// Appends `src` vertices and triangles into `dst` with offset triangle indices.
fn append_mesh(dst: &mut SurfaceMesh, src: &SurfaceMesh) {
    let base = dst.vertices.len() as u32;
    dst.vertices.extend_from_slice(&src.vertices);
    dst.normals.extend_from_slice(&src.normals);
    dst.nearest_atom.extend_from_slice(&src.nearest_atom);
    for &[a, b, c] in &src.triangles {
        dst.triangles.push([base + a, base + b, base + c]);
    }
}

#[derive(Copy, Clone, PartialEq)]
struct BottleneckState {
    radius: f32,
    idx: usize,
}

impl Eq for BottleneckState {}

impl Ord for BottleneckState {
    fn cmp(&self, other: &Self) -> Ordering {
        self.radius.total_cmp(&other.radius).then_with(|| self.idx.cmp(&other.idx))
    }
}

impl PartialOrd for BottleneckState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Computes the widest-path bottleneck radius (Å) from `exterior_seed` to every grid voxel.
fn compute_bottleneck_radii(
    phi_a: &[f32],
    phi_b: &[f32],
    exterior_seed: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<f32> {
    let total = nx * ny * nz;
    let stride_y = nz;
    let stride_x = ny * nz;
    let mut best = vec![0.0_f32; total];
    let mut heap = BinaryHeap::new();

    for g in 0..total {
        if exterior_seed[g] {
            let r = phi_a[g].min(phi_b[g]).max(0.0);
            if r > 0.0 {
                best[g] = r;
                heap.push(BottleneckState { radius: r, idx: g });
            }
        }
    }

    while let Some(BottleneckState { radius, idx }) = heap.pop() {
        if radius < best[idx] {
            continue;
        }
        let ix = idx / stride_x;
        let rem = idx % stride_x;
        let iy = rem / stride_y;
        let iz = rem % stride_y;

        let mut relax = |nx_i: usize, ny_i: usize, nz_i: usize| {
            let ng = nx_i * stride_x + ny_i * stride_y + nz_i;
            let clearance = phi_a[ng].min(phi_b[ng]);
            let cand = radius.min(clearance);
            if cand > best[ng] {
                best[ng] = cand;
                heap.push(BottleneckState { radius: cand, idx: ng });
            }
        };

        if ix > 0 { relax(ix - 1, iy, iz); }
        if ix + 1 < nx { relax(ix + 1, iy, iz); }
        if iy > 0 { relax(ix, iy - 1, iz); }
        if iy + 1 < ny { relax(ix, iy + 1, iz); }
        if iz > 0 { relax(ix, iy, iz - 1); }
        if iz + 1 < nz { relax(ix, iy, iz + 1); }
    }

    best
}
