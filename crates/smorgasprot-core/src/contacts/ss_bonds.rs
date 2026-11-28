//! Disulfide (S–S) bond detection.
//!
//! ## Summary
//! Identifies covalent disulfide bridges between cysteine `SG` atoms across residues using
//! distance, bond-angle, and dihedral-angle constraints. The primary entry point is
//! [`detect_ss_bonds`], configured by [`SsBondParams`]. Disulfide geometry is surprisingly
//! constrained due to lone-pair repulsion and "hyperconjugation".
//!
//! ## Considerations
//! - **Cysteine `SG` specificity**: Only sulfur atoms named `SG` are considered, preventing
//!   methionine `SD` atoms from being misclassified as disulfides.
//! - **Distance**: Canonical S–S bond length is ~2.03 Å; [`SsBondParams`] allows a configurable
//!   window (`min_ss_dist_a`..=`max_ss_dist_a`) to accommodate refinement noise while rejecting
//!   non-bonded contacts or severe clashes.
//! - **Bond angle (`Cβ–Sγ–Sγ'`)**: Lone-pair repulsion compresses the angle around sulfur to
//!   ~104° (Petersen et al., 1999). Both endpoints are checked against
//!   `min_cb_sg_sg_deg`..=`max_cb_sg_sg_deg`.
//! - **Dihedral (`|χ_SS|` = `|Cβ–Sγ–Sγ'–Cβ'|`)**: Stereoelectronic lone-pair repulsion and
//!   hyperconjugation favor a bimodal distribution around ±90°. Taking the absolute value treats
//!   right-handed (+90°) and left-handed (−90°) disulfides uniformly.
//! - **Missing `Cβ` fallback**: If either residue lacks a `Cβ` atom (e.g., incomplete model),
//!   angle/dihedral checks are skipped for that pair and distance alone decides.
//!
//! ## References
//! - Petersen, M. T., Jonson, P. H., Petersen, S. B. (1999). Amino acid neighbours and detailed
//!   conformational analysis of cysteines in proteins. https://doi.org/10.1093/protein/12.7.535
//!
//! ## TODO
//! - Optionally classify right- vs. left-handed / allosteric disulfide conformations if needed.

use glam::Vec3;

use crate::model::{Bond, BondKind, BondOrder, ComplexStructure, Element};
use crate::spatial::{SpatialGrid, bond_angle_deg, dihedral_deg};

crate::param_group! {
    /// Tunables for disulfide bond detection (`detect_ss_bonds`).
    #[desc = "Parameters controlling disulfide (SG–SG) bond detection."]
    pub struct SsBondParams("ss_bonds", "Disulfide Bond Detection") {
        /// Minimum S–S distance (Å) to avoid classifying severe clashes as disulfides.
        #[desc = "Minimum SG–SG distance to be considered a disulfide bond. 2.02 ± 0.10 Å is generous padding."]
        min_ss_dist_a: f32 = 1.92, 0.5..=2.0, "Min S–S distance", "Å";
        /// Maximum S–S distance (Å) for a disulfide bridge (canonical ~2.03 Å).
        #[desc = "Maximum SG–SG distance to be considered a disulfide bond. 2.02 ± 0.10 Å is generous padding."]
        max_ss_dist_a: f32 = 2.12, 2.0..=3.5, "Max S–S distance", "Å";
        /// Minimum Cβ–Sγ–Sγ' bond angle (degrees, canonical ~104°).
        #[desc = "Minimum Cβ–Sγ–Sγ' bond angle. 105 ± 15 ° is generous padding."]
        min_cb_sg_sg_deg: f32 = 90.0, 45.0..=105.0, "Min Cβ–Sγ–Sγ' angle", "°";
        /// Maximum Cβ–Sγ–Sγ' bond angle (degrees, canonical ~104°).
        #[desc = "Maximum Cβ–Sγ–Sγ' bond angle. 105 ± 15 ° is generous padding."]
        max_cb_sg_sg_deg: f32 = 120.0, 105.0..=165.0, "Max Cβ–Sγ–Sγ' angle", "°";
        /// Minimum |χ_SS| dihedral angle (degrees, canonical ±90°).
        #[desc = "Minimum |Cβ–Sγ–Sγ'–Cβ'| dihedral angle."]
        min_abs_chi_ss_deg: f32 = 45.0, 0.0..=90.0, "Min |χ_SS| dihedral", "°";
        /// Maximum |χ_SS| dihedral angle (degrees, canonical ±90°).
        #[desc = "Maximum |Cβ–Sγ–Sγ'–Cβ'| dihedral angle."]
        max_abs_chi_ss_deg: f32 = 135.0, 90.0..=180.0, "Max |χ_SS| dihedral", "°";
    }
}

/// Detects inter-residue disulfide (`SG–SG`) bonds satisfying distance, `Cβ–Sγ–Sγ'` bond angle,
/// and `|χ_SS|` dihedral constraints.
pub fn detect_ss_bonds(complex: &ComplexStructure, p: &SsBondParams) -> Vec<Bond> {
    let mut bonds = Vec::new();
    let min_sq = p.min_ss_dist_a * p.min_ss_dist_a;
    let max_sq = p.max_ss_dist_a * p.max_ss_dist_a;

    // 1. Index all cysteine SG sulfur atoms in a spatial grid.
    let mut grid = SpatialGrid::new(p.max_ss_dist_a);
    for (i, atom) in complex.atoms.iter().enumerate() {
        if atom.element == Element::S && &atom.atom_name == b"SG  " {
            grid.insert(i as u32, atom.pos_3d);
        }
    }

    // 2. Query pairs of SG atoms across different residues and validate geometry.
    for (i, atom_a) in complex.atoms.iter().enumerate() {
        if atom_a.element != Element::S || &atom_a.atom_name != b"SG  " {
            continue;
        }
        grid.query_radius(atom_a.pos_3d, p.max_ss_dist_a, |j, _pos, dist_sq| {
            let j = j as usize;
            if j <= i || dist_sq < min_sq || dist_sq > max_sq {
                return;
            }
            let atom_b = &complex.atoms[j];
            if atom_a.residue_idx == atom_b.residue_idx {
                return;
            }

            // 3. Enforce Cβ–Sγ–Sγ' bond angles (~104°) and |χ_SS| dihedral (~90°) when Cβ exists.
            if let (Some(cb_a), Some(cb_b)) = (
                complex.residue_atom(atom_a.residue_idx, "CB"),
                complex.residue_atom(atom_b.residue_idx, "CB"),
            ) {
                let p_cb_a = Vec3::from_array(cb_a.pos_3d);
                let p_sg_a = Vec3::from_array(atom_a.pos_3d);
                let p_sg_b = Vec3::from_array(atom_b.pos_3d);
                let p_cb_b = Vec3::from_array(cb_b.pos_3d);

                let Some(ang_a) = bond_angle_deg(p_cb_a, p_sg_a, p_sg_b) else {
                    return;
                };
                let Some(ang_b) = bond_angle_deg(p_cb_b, p_sg_b, p_sg_a) else {
                    return;
                };
                if ang_a < p.min_cb_sg_sg_deg
                    || ang_a > p.max_cb_sg_sg_deg
                    || ang_b < p.min_cb_sg_sg_deg
                    || ang_b > p.max_cb_sg_sg_deg
                {
                    return;
                }

                let abs_chi = dihedral_deg(p_cb_a, p_sg_a, p_sg_b, p_cb_b).abs();
                if abs_chi < p.min_abs_chi_ss_deg || abs_chi > p.max_abs_chi_ss_deg {
                    return;
                }
            }

            bonds.push(Bond {
                a: atom_a.idx,
                b: atom_b.idx,
                kind: BondKind::Disulfide,
                order: BondOrder::Single,
            });
        });
    }

    bonds
}
