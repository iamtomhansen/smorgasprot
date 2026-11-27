//! Covalent bond detection within a molecular structure.
//! 
//! ## Summary
//! Derives covalent bonds from standard molecule features (CCD recipes, backbone connectivity),
//! explicit bonds in the input structure, and geometric perception.
//!
//! ## Considerations
//! The full chemical component dictionary (CCD) is too large to include in this crate and is
//! probably overkill for now. Instead, a table of amino acid bonds is used.
//! Geometric perception is a more general approach, with per-bond-type modules (h_bonds, ss_bonds,
//! etc.) that can be made surprisingly robust with a few heuristics. Atom mobility and refinement
//! noise are accounted for through parametrized B-factor scaling of tolerances.
//!
//! ## TODO
//! - Submit a PR to `pdbtbx` adding full legacy bond parsing (e.g. CONECT records)

use super::amino_acid_bonds::residue_bonds;
use super::ss_bonds::{SsBondParams, detect_ss_bonds};
use crate::amino_acids::ElementExt;
use crate::model::{AtomRecord, Bond, BondKind, BondOrder, ComplexStructure, Element};
use crate::physics::is_adjacent_backbone_pair;
use crate::spatial::SpatialGrid;

crate::param_group! {
    /// Tunables for geometric covalent-bond detection (`detect_bonds`).
    #[desc = "Paramaters affectinv the stringency of geometry-based covalent-bond detection."]
    pub struct BondParams("bonds", "Covalent Bond Detection") {
        /// Accounts for thermal motion and refinement noise in well-ordered regions.
        #[desc = "Extra tolerance added to the sum of two covalent radii before deeming a pair bonded."]
        tolerance_a: f32 = 0.45, 0.0..=1.0, "Bond tolerance", "Å";
        /// Scale factor on `sqrt(avg B-factor)`, added to the tolerance. Kept conservative by
        /// default: better to miss some covalent bonds than to assume everything is bonded.
        #[desc = "Use B-factor to scale bond tolerance in cases where noisy/mobile structures might otherwise miss a bond."]
        b_factor_scale: f32 = 0.015, 0.0..=0.2, "B-factor tolerance scale", "Å/√B";
        /// Upper bound on how much B-factor can widen the tolerance.
        #[desc = "An upper limit to the B-factor contribution to bond tolerance."]
        b_factor_scale_max_a: f32 = 0.25, 0.0..=1.0, "Max B-factor tolerance addition", "Å";
        /// Maximum C_i–N_{i+1} distance to form an inter-residue peptide bond (respects chain breaks).
        #[desc = "Maximum C-N distance for an inter-residue peptide bond."]
        max_peptide_bond_a: f32 = 2.0, 1.0..=3.0, "Max peptide bond length", "Å";
    }
}

/// Upper bound (Å) on any single covalent bond this project specifically detects, sizing the
/// neighbor grid search. Twice sulfur's covalent radius plus the max allowed tolerance.
fn max_bond_search_a(p: &BondParams) -> f32 {
    2.0 * Element::S.covalent_radius_a() + p.tolerance_a + p.b_factor_scale_max_a
}

/// Extra tolerance (Å) from thermal/positional uncertainty, scaling with `sqrt` of the pair's
/// average B-factor and capped by `p.b_factor_scale_max_a`.
fn b_factor_tolerance_a(atom_a: &AtomRecord, atom_b: &AtomRecord, p: &BondParams) -> f32 {
    let avg_b = (0.5 * (atom_a.b_factor + atom_b.b_factor)).max(0.0);
    (p.b_factor_scale * avg_b.sqrt()).min(p.b_factor_scale_max_a)
}

/// Returns true for the 5 invariant polypeptide backbone atom names. OXT = C-terminal oxygen.
fn is_backbone_atom_name(name: &str) -> bool {
    matches!(name, "N" | "CA" | "C" | "O" | "OXT")
}

/// Detects covalent bonds across the whole structure:
/// 1. **Tier 1 (Topology-first)**: Standard amino acid intra-residue templates
///    ([`residue_bonds`]) and sequence-adjacent peptide bonds (`C–N` within `p.max_peptide_bond_a`).
/// 2. **Tier 2 (Geometric fallback)**: Distance-based perception for non-template residues
///    (ligands, HETATMs, unknowns).
/// 3. **Disulfides**: Dedicated `SG–SG` distance, angle, and dihedral perception via
///    [`detect_ss_bonds`].
pub fn detect_bonds(complex: &ComplexStructure, p: &BondParams) -> Vec<Bond> {
    let mut bonds = Vec::new();
    let mut covered_residues = vec![false; complex.residues.len()];

    // 1. Tier 1a: Intra-residue bonds from static amino-acid templates.
    for (res_idx, res) in complex.residues.iter().enumerate() {
        let res_name = std::str::from_utf8(&res.res_name).unwrap_or("").trim();
        let Some(template) = residue_bonds(res_name) else {
            continue;
        };
        covered_residues[res_idx] = true;

        for tmpl in template {
            if let (Some(atom_a), Some(atom_b)) = (
                complex.residue_atom(res.idx, tmpl.atom_a),
                complex.residue_atom(res.idx, tmpl.atom_b),
            ) {
                let kind = if is_backbone_atom_name(tmpl.atom_a)
                    && is_backbone_atom_name(tmpl.atom_b)
                {
                    BondKind::Backbone
                } else {
                    BondKind::Sidechain
                };
                bonds.push(Bond { a: atom_a.idx, b: atom_b.idx, kind, order: tmpl.order });
            }
        }

        // Handle C-terminal OXT if present.
        if let (Some(c_atom), Some(oxt_atom)) = (
            complex.residue_atom(res.idx, "C"),
            complex.residue_atom(res.idx, "OXT"),
        ) {
            bonds.push(Bond {
                a: c_atom.idx,
                b: oxt_atom.idx,
                kind: BondKind::Backbone,
                order: BondOrder::Single,
            });
        }
    }

    // 2. Tier 1b: Polymeric peptide bonds (C_i -> N_{i+1}) for adjacent residues.
    let max_peptide_sq = p.max_peptide_bond_a * p.max_peptide_bond_a;
    for pair in complex.residues.windows(2) {
        let (res_a, res_b) = (&pair[0], &pair[1]);
        if !is_adjacent_backbone_pair(res_a, res_b) {
            continue;
        }
        if let (Some(c_atom), Some(n_atom)) = (
            complex.residue_atom(res_a.idx, "C"),
            complex.residue_atom(res_b.idx, "N"),
        ) {
            let a = c_atom.pos_3d;
            let b = n_atom.pos_3d;
            let d2 = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
            if d2 <= max_peptide_sq {
                bonds.push(Bond {
                    a: c_atom.idx,
                    b: n_atom.idx,
                    kind: BondKind::Backbone,
                    order: BondOrder::Single,
                });
            }
        }
    }

    // 3. Tier 2: Geometric fallback for non-template residues and ligands.
    let search_radius = max_bond_search_a(p);
    let mut grid = SpatialGrid::new(search_radius);
    for (i, atom) in complex.atoms.iter().enumerate() {
        grid.insert(i as u32, atom.pos_3d);
    }

    for (i, atom_a) in complex.atoms.iter().enumerate() {
        let res_a_idx = atom_a.residue_idx.0 as usize;
        let res_a = &complex.residues[res_a_idx];
        grid.query_radius(atom_a.pos_3d, search_radius, |j, _pos, dist_sq| {
            let j = j as usize;
            if j <= i {
                return;
            }
            let atom_b = &complex.atoms[j];
            let res_b_idx = atom_b.residue_idx.0 as usize;

            // Skip pairs where both residues are already covered by Tier 1 templates.
            if covered_residues[res_a_idx] && covered_residues[res_b_idx] {
                return;
            }

            let tolerance = p.tolerance_a + b_factor_tolerance_a(atom_a, atom_b, p);
            let max_bond_dist =
                atom_a.element.covalent_radius_a() + atom_b.element.covalent_radius_a() + tolerance;
            if dist_sq > max_bond_dist * max_bond_dist {
                return;
            }

            let res_b = &complex.residues[res_b_idx];
            let kind = if atom_a.residue_idx == atom_b.residue_idx {
                let name_a = std::str::from_utf8(&atom_a.atom_name).unwrap_or("").trim();
                let name_b = std::str::from_utf8(&atom_b.atom_name).unwrap_or("").trim();
                if is_backbone_atom_name(name_a) && is_backbone_atom_name(name_b) {
                    BondKind::Backbone
                } else {
                    BondKind::Sidechain
                }
            } else if is_adjacent_backbone_pair(res_a, res_b) {
                BondKind::Backbone
            } else {
                BondKind::Other
            };
            bonds.push(Bond { a: atom_a.idx, b: atom_b.idx, kind, order: BondOrder::Single });
        });
    }

    // 4. Disulfide bridges (geometry-validated SG–SG pairs).
    bonds.extend(detect_ss_bonds(complex, &SsBondParams::default()));

    bonds
}
