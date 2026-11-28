//! Salt-bridge (electrostatic ion-pair) detection.
//!
//! ## Summary
//! Identifies non-covalent salt bridges between oppositely charged sidechain functional atoms
//! within a distance cutoff (`salt_bridge_max_dist_a`, default `4.0 Å`). The primary entry point
//! is [`is_salt_bridge`].
//!
//! ## Considerations
//! - **Distance-based criterion**: Following Barlow & Thornton (1983), salt bridges are defined
//!   by a `≤ 4.0 Å` heavy-atom distance between oppositely charged functional groups (`LYS`/`ARG`/
//!   `HIS` vs. `ASP`/`GLU`), without an explicit angular cutoff (unlike hydrogen bonds).
//! - **Metal ions**: `is_positive_sidechain_atom` currently includes `element.is_metal_ion()` as
//!   a fallback until dedicated metal-coordination logic is implemented in
//!   [`crate::interactions::coordinations`].
//! - **Parameters**: Currently reads `salt_bridge_max_dist_a` from [`ContactParams`] in
//!   `physics.rs`.
//!
//! ## References
//! - Barlow, D. J., Thornton, J. M. (1983). Ion-pairs in proteins.
//!   https://doi.org/10.1016/S0022-2836(83)80284-8
//!
//! ## TODO
//! - Route metal–carboxylate interactions to [`crate::interactions::coordinations`] once
//!   implemented.

use crate::amino_acids::ElementExt;
use crate::model::{AtomRecord, ResidueRecord};
use crate::physics::ContactParams;

/// Returns true if `atom_a` and `atom_b` carry opposite formal charges within
/// `p.salt_bridge_max_dist_a` (Barlow & Thornton, 1983).
#[inline]
pub fn is_salt_bridge(
    atom_a: &AtomRecord,
    res_a: &ResidueRecord,
    atom_b: &AtomRecord,
    res_b: &ResidueRecord,
    dist_a: f32,
    p: &ContactParams,
) -> bool {
    dist_a <= p.salt_bridge_max_dist_a && is_opposite_charged_atoms(atom_a, res_a, atom_b, res_b)
}

fn is_positive_sidechain_atom(atom: &AtomRecord, res: &ResidueRecord) -> bool {
    if let Some(aa) = res.kind.amino_acid() {
        let aname = std::str::from_utf8(&atom.atom_name).unwrap_or("");
        aa.is_positive_functional_atom(aname)
    } else {
        atom.element.is_metal_ion()
    }
}

fn is_negative_sidechain_atom(atom: &AtomRecord, res: &ResidueRecord) -> bool {
    if let Some(aa) = res.kind.amino_acid() {
        let aname = std::str::from_utf8(&atom.atom_name).unwrap_or("");
        aa.is_negative_functional_atom(aname)
    } else {
        false
    }
}

fn is_opposite_charged_atoms(
    atom_a: &AtomRecord,
    res_a: &ResidueRecord,
    atom_b: &AtomRecord,
    res_b: &ResidueRecord,
) -> bool {
    (is_positive_sidechain_atom(atom_a, res_a) && is_negative_sidechain_atom(atom_b, res_b))
        || (is_negative_sidechain_atom(atom_a, res_a) && is_positive_sidechain_atom(atom_b, res_b))
}
