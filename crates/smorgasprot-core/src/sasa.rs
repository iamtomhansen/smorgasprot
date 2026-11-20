//! Shrake-Rupley solvent-accessible surface area (ΔSASA) calculation.
//! [Information]: https://en.wikipedia.org/wiki/Accessible_surface_area#Shrake-Rupley_algorithm
//!
//! ## Works referenced
//! 1. **Rolling-Probe Solvent-Accessible Surface Definition ($r_{\text{probe}} = 1.4\text{ \AA}$):**
//!    Lee, B., & Richards, F. M. (1971). The interpretation of protein structures: estimation of
//!    static accessibility. *Journal of Molecular Biology*, 55(3), 379–400.
//! 2. **Discrete Sphere Test-Point SASA Integration:**
//!    Shrake, A., & Rupley, J. A. (1973). Environment and exposure to solvent of protein atoms.
//!    Lysozyme and insulin. *Journal of Molecular Biology*, 79(2), 351–371.
//! 3. **Golden-Angle Fibonacci Sphere Quadrature (as used in FreeSASA):**
//!    González, Á. (2010). Measurement of Areas on a Sphere Using Fibonacci and Latitude–Longitude
//!    Lattices. *Mathematical Geosciences*, 42(1), 49–64; Mitternacht, S. (2016). FreeSASA: An open
//!    source C library for solvent accessible surface area calculations. *BMC Bioinformatics*, 17, 18.

use crate::model::{ComplexStructure, InterfaceSide};
use crate::spatial::SpatialGrid;
use glam::Vec3;

/// Radius of a water probe sphere used to evaluate solvent accessibility (in Angstroms).
const PROBE_RADIUS_A: f32 = 1.4;
/// Number of golden-angle Fibonacci test points sampled on each atom's expanded sphere.
/// Higher values yield more accurate SASA estimates at the cost of linearly increased runtime.
/// 72 points yields ~1% relative error.
const FIBONACCI_POINTS: usize = 72;
/// Voxel bucket width for the 3D spatial hash grid (in Angstroms).
/// Setting this equal to the maximum two-sphere occlusion reach (~6.4 A) ensures
/// `SpatialGrid::query_radius` checks at most `reach = 1` (a 3x3x3 = 27-voxel neighborhood).
const GRID_RESOLUTION_A: f32 = 6.4;

/// Per-atom SASA metrics in Angstroms^2.
#[derive(Debug, Clone, Copy, Default)]
pub struct AtomSasa {
    /// Solvent-accessible surface area of the atom in its isolated partner (`SideA` or `SideB` alone).
    pub free_sasa_a2: f32,
    /// Solvent-accessible surface area of the atom in the bound `SideA` + `SideB` complex.
    pub complex_sasa_a2: f32,
    /// Surface area lost upon complex formation (`free_sasa_a2 - complex_sasa_a2`).
    pub buried_sasa_a2: f32,
    /// Shortest 3D center-to-center distance to any heavy atom on the opposite interface side.
    pub min_cross_dist_a: f32,
}

/// Generates `n` uniformly distributed unit vectors on the sphere via a golden-angle Fibonacci lattice.
fn fibonacci_sphere(n: usize) -> Vec<Vec3> {
    let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - (2.0 * (i as f32 + 0.5)) / (n as f32);
            let r = (1.0 - y * y).max(0.0).sqrt();
            let theta = golden_angle * (i as f32);
            Vec3::new(r * theta.cos(), y, r * theta.sin())
        })
        .collect()
}

/// Computes `AtomSasa` (unbound SASA, bound SASA, ΔSASA, and closest cross-side distance)
/// for all atoms assigned to `SideA` or `SideB` in `complex`, ignoring `Unassigned` (`InterfaceSide::None`) chains.
pub fn compute_complex_sasa(complex: &ComplexStructure) -> Vec<AtomSasa> {
    let n_atoms = complex.atoms.len();
    let mut results = vec![
        AtomSasa {
            min_cross_dist_a: f32::INFINITY,
            ..Default::default()
        };
        n_atoms
    ];

    // 1. Index all assigned (SideA and SideB) heavy atoms into a 3D spatial grid.
    let mut grid = SpatialGrid::new(GRID_RESOLUTION_A);
    for (i, atom) in complex.atoms.iter().enumerate() {
        if atom.role.is_assigned() {
            grid.insert(i as u32, atom.pos_3d);
        }
    }

    // Precompute unit sphere test directions and the solid-angle weight (4π / N) per test point.
    let sphere_pts = fibonacci_sphere(FIBONACCI_POINTS);
    let weight_per_pt = 4.0 * std::f32::consts::PI / (FIBONACCI_POINTS as f32);

    let mut same_neighbors: Vec<(Vec3, f32)> = Vec::with_capacity(64);
    let mut cross_neighbors: Vec<(Vec3, f32)> = Vec::with_capacity(64);

    for (i, atom) in complex.atoms.iter().enumerate() {
        let side_i = atom.role;
        if !side_i.is_assigned() {
            continue;
        }

        let pos_i = Vec3::from(atom.pos_3d);
        // Solvent-expanded radius R_i = r_vdW + r_probe
        let r_i = atom.vdw_radius_a + PROBE_RADIUS_A;
        // Upper bound on distance at which a neighbor's expanded sphere R_j can intersect R_i
        let query_reach = r_i + 1.85 + PROBE_RADIUS_A;

        same_neighbors.clear();
        cross_neighbors.clear();
        let mut min_cross_d2 = f32::INFINITY;

        // 2. Partition nearby atoms into same-side neighbors (intra-partner occlusion)
        //    and opposite-side neighbors (cross-interface occlusion).
        grid.query_radius(atom.pos_3d, query_reach, |j_u32, pos_j_arr, d2| {
            let j = j_u32 as usize;
            if i == j {
                return;
            }
            let other = &complex.atoms[j];
            let side_j = other.role;
            if !side_j.is_assigned() {
                return;
            }
            let r_j = other.vdw_radius_a + PROBE_RADIUS_A;
            let max_d = r_i + r_j;
            if side_j != side_i && d2 < min_cross_d2 {
                min_cross_d2 = d2;
            }
            if d2 < max_d * max_d {
                let pos_j = Vec3::from(pos_j_arr);
                if side_j == side_i {
                    same_neighbors.push((pos_j, r_j * r_j));
                } else {
                    cross_neighbors.push((pos_j, r_j * r_j));
                }
            }
        });

        let min_cross_dist_a = min_cross_d2.sqrt();
        results[i].min_cross_dist_a = min_cross_dist_a;

        // Fast skip: if no opposite-side expanded sphere touches R_i, ΔSASA is identically 0.
        if cross_neighbors.is_empty() && min_cross_dist_a > 6.5 {
            continue;
        }

        let mut free_exposed = 0usize;
        let mut complex_exposed = 0usize;

        // 3. Test each point on atom i's expanded sphere against same-side and cross-side neighbors:
        'pts: for &u in &sphere_pts {
            let test_pt = pos_i + r_i * u;

            // (a) Intra-partner burial check: if occluded by an atom on the SAME side, this surface
            //     patch is already buried inside the isolated partner's own interior and cannot
            //     contribute to interface ΔSASA.
            for &(nbr_pos, r_j_sq) in &same_neighbors {
                if test_pt.distance_squared(nbr_pos) < r_j_sq {
                    continue 'pts;
                }
            }

            // (b) The point is solvent-exposed in the unbound partner (free state).
            free_exposed += 1;

            // (c) Cross-interface burial check: test whether binding the opposite partner occludes
            //     this previously solvent-exposed point.
            let mut occluded_by_partner = false;
            for &(nbr_pos, r_j_sq) in &cross_neighbors {
                if test_pt.distance_squared(nbr_pos) < r_j_sq {
                    occluded_by_partner = true;
                    break;
                }
            }
            if !occluded_by_partner {
                complex_exposed += 1;
            }
        }

        // 4. Convert point counts to surface areas (4π R_i^2 * count / N).
        //    Note that `buried_sasa` (ΔSASA = free_sasa - complex_sasa) counts ONLY points that
        //    were exposed on the isolated partner in step (b) and occluded by the opposite partner in step (c).
        let area_scale = r_i * r_i * weight_per_pt;
        let free_sasa = (free_exposed as f32) * area_scale;
        let complex_sasa = (complex_exposed as f32) * area_scale;
        let buried_sasa = (free_sasa - complex_sasa).max(0.0);

        results[i].free_sasa_a2 = free_sasa;
        results[i].complex_sasa_a2 = complex_sasa;
        results[i].buried_sasa_a2 = buried_sasa;
    }

    results
}

/// Returns true if an atom qualifies as part of the interface patch.
pub fn is_interfacial_atom(_side: InterfaceSide, sasa: &AtomSasa) -> bool {
    sasa.buried_sasa_a2 > 0.15 || sasa.min_cross_dist_a <= 4.5
}
