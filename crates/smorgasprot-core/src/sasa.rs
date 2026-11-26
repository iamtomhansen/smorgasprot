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

use crate::interface::definition::InterfacePartition;
use crate::model::ComplexStructure;
use crate::spatial::SpatialGrid;
use glam::Vec3;

crate::param_group! {
    /// Shrake-Rupley ΔSASA.
    #[desc = "Calculates solvent-accessible surface area loss upon complex formation via Shrake-Rupley sphere quadrature."]
    pub struct SasaParams("sasa", "Solvent Accessibility (ΔSASA)") {
        /// Water probe radius added to each van der Waals radius (Å).
        #[desc = "Effective radius of a solvent water probe sphere rolling over the molecular surface."]
        probe_radius_a: f32 = 1.4, 0.5..=3.0, "Probe radius", "Å";
        /// Fibonacci test points per atom sphere; 72 gives about 1 % error.
        #[desc = "Number of golden-angle Fibonacci lattice points distributed on each atom's solvent-expanded sphere."]
        sphere_points: usize = 72, 12..=1000, "Sphere test points", "";
        /// Spatial hash bucket width (Å); affects speed only.
        #[desc = "Bucket width for the uniform voxel grid used to accelerate neighbor search. Affects calculation speed, not results."]
        grid_resolution_a: f32 = 6.4, 2.0..=12.0, "Spatial grid cell size", "Å";
    }
}

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
/// for all atoms assigned to `SideA` or `SideB` in `partition`, ignoring `Unassigned` chains.
pub fn compute_complex_sasa(
    complex: &ComplexStructure,
    partition: &InterfacePartition,
    p: &SasaParams,
) -> Vec<AtomSasa> {
    let n_atoms = complex.atoms.len();
    let mut results = vec![
        AtomSasa {
            min_cross_dist_a: f32::INFINITY,
            ..Default::default()
        };
        n_atoms
    ];

    let atom_sides = partition.atom_sides(complex);

    // 1. Index all assigned (SideA and SideB) heavy atoms into a 3D spatial grid.
    let mut grid = SpatialGrid::new(p.grid_resolution_a);
    for (i, atom) in complex.atoms.iter().enumerate() {
        if atom_sides[i].is_assigned() {
            grid.insert(i as u32, atom.pos_3d);
        }
    }

    // Precompute unit sphere test directions and the solid-angle weight (4π / N) per test point.
    let sphere_pts = fibonacci_sphere(p.sphere_points);
    let weight_per_pt = 4.0 * std::f32::consts::PI / (p.sphere_points as f32);

    let mut same_neighbors: Vec<(Vec3, f32)> = Vec::with_capacity(64);
    let mut cross_neighbors: Vec<(Vec3, f32)> = Vec::with_capacity(64);

    for (i, atom) in complex.atoms.iter().enumerate() {
        let side_i = atom_sides[i];
        if !side_i.is_assigned() {
            continue;
        }

        let pos_i = Vec3::from(atom.pos_3d);
        // Solvent-expanded radius R_i = r_vdW + r_probe
        let r_i = atom.vdw_radius_a + p.probe_radius_a;
        // Upper bound on distance at which a neighbor's expanded sphere R_j can intersect R_i
        let query_reach = r_i + 1.85 + p.probe_radius_a;

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
            let side_j = atom_sides[j];
            if !side_j.is_assigned() {
                return;
            }
            let r_j = other.vdw_radius_a + p.probe_radius_a;
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
