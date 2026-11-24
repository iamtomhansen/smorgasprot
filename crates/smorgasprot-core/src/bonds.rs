//! Covalent bond detection: a whole-structure connectivity graph computed once from geometry. This
//! approach (`dist <= r_cov(a) + r_cov(b) + tolerance`) is the conventional method used by most
//! other molecular toolkits to infer bonds.
//!
//! Covalent radii come from `pdbtbx::Element::atomic_radius().covalent_single`.
//!
//! TODO: structures from generative/inverse-folding models (RFdiffusion, AlphaProteo, LatentX,
//! ProteinMPNN, etc.) can generate intersecting atoms with a clash severe enough to land inside
//! covalent bond range. Nothing here currently distinguishes that from a real bond. Will need to
//! write some more robust logic beyond distance-based detection.

use crate::amino_acids::ElementExt;
use crate::model::{AtomRecord, Bond, BondKind, ComplexStructure, Element};
use crate::physics::is_adjacent_backbone_pair;
use crate::spatial::SpatialGrid;

crate::param_group! {
    /// Tunables for geometric covalent-bond detection (`detect_bonds`).
    pub struct BondParams("bonds", "Covalent Bond Detection") {
        /// Extra allowance added to the sum of two covalent radii before deeming a pair bonded.
        /// Accounts for thermal motion and refinement noise in well-ordered regions.
        tolerance_a: f32 = 0.45, 0.0..=1.0, "Bond tolerance", "Å";
        /// Scale factor on `sqrt(avg B-factor)`, added to the tolerance. Kept conservative by
        /// default: better to miss some covalent bonds than to assume everything is bonded.
        b_factor_scale: f32 = 0.015, 0.0..=0.2, "B-factor tolerance scale", "Å/√B";
        /// Upper bound on how much B-factor can widen the tolerance.
        b_factor_scale_max_a: f32 = 0.25, 0.0..=1.0, "Max B-factor tolerance addition", "Å";
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
fn is_backbone_atom_name(atom_name: &[u8; 4]) -> bool {
    let name = std::str::from_utf8(atom_name).unwrap_or("").trim();
    matches!(name, "N" | "CA" | "C" | "O" | "OXT")
}

/// Detects covalent bonds across the whole structure via distance-based geometric perception.
///
/// Note: For a well-formed structure, a severe steric clash is never misread as a bond: carbon's
/// covalent radius is ~0.76 Å, so a C-C pair bonds at `dist <= 1.97 Å` (after default
/// `p.tolerance_a`). `physics::check_steric_clash` already flags *any* clash below
/// `dist < 3.00 Å` (carbon's ~1.70 Å van der Waals radius, less the clash tolerance).
/// See module-level TODO for poorly formed structures (e.g. generative models).
pub fn detect_bonds(complex: &ComplexStructure, p: &BondParams) -> Vec<Bond> {
    let search_radius = max_bond_search_a(p);
    let mut grid = SpatialGrid::new(search_radius);
    for (i, atom) in complex.atoms.iter().enumerate() {
        grid.insert(i as u32, atom.pos_3d);
    }

    let mut bonds = Vec::new();
    for (i, atom_a) in complex.atoms.iter().enumerate() {
        let res_a = &complex.residues[atom_a.residue_idx.0 as usize];
        grid.query_radius(atom_a.pos_3d, search_radius, |j, _pos, dist_sq| {
            let j = j as usize;
            if j <= i {
                // Each unordered pair is visited twice (once from each endpoint); keep the i < j half.
                return;
            }
            let atom_b = &complex.atoms[j];
            let tolerance = p.tolerance_a + b_factor_tolerance_a(atom_a, atom_b, p);
            let max_bond_dist = atom_a.element.covalent_radius_a() + atom_b.element.covalent_radius_a() + tolerance;
            if dist_sq > max_bond_dist * max_bond_dist {
                return;
            }
            let res_b = &complex.residues[atom_b.residue_idx.0 as usize];
            let kind = if atom_a.element == Element::S && atom_b.element == Element::S {
                BondKind::Disulfide
            } else if atom_a.residue_idx == atom_b.residue_idx {
                if is_backbone_atom_name(&atom_a.atom_name) && is_backbone_atom_name(&atom_b.atom_name) {
                    BondKind::Backbone
                } else {
                    BondKind::Sidechain
                }
            } else if is_adjacent_backbone_pair(res_a, res_b) {
                BondKind::Backbone
            } else {
                BondKind::Other
            };
            bonds.push(Bond { a: atom_a.idx, b: atom_b.idx, kind });
        });
    }
    bonds
}
