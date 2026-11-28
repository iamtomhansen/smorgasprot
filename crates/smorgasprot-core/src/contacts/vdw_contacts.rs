//! Steric clash and non-bonded van der Waals (vdW) proximity contact detection.
//!
//! ## Summary
//! Evaluates whether two non-bonded heavy atoms violate van der Waals steric exclusion
//! ([`check_steric_clash`]), and provides the conceptual boundary for non-specific
//! `VanDerWaalsContact` edges within the first-shell cutoff (`first_shell_cutoff_a`, default
//! `4.5 Å`).
//!
//! ## Considerations
//! In physical chemistry, "van der Waals forces" can encompass London dispersion, Debye induction,
//! and Keesom dipole–dipole interactions. Here, "vdW contact" is used as shorthand for non-bonded
//! heavy-atom proximity (pairs within the first-shell cutoff that are neither clashing nor forming
//! other bonds/interactions).
//! - **Hydrogen-bond clash allowance**: Following Word et al. (1999) (`Probe`/MolProbity),
//!   polar `N`/`O` pairs receive a larger overlap allowance (`hbond_clash_tolerance_a`) than
//!   apolar pairs (`clash_tolerance_a`) to reflect implicit hydrogen-bond shortening.
//! - **Parameters**: Currently reads `clash_tolerance_a` and `hbond_clash_tolerance_a` from
//!   [`ContactParams`] in `physics.rs`.
//!
//! ## References
//! - Word, J. M., Lovell, S. C., LaBean, T. H. et al. (1999). Visualizing and quantifying
//!   molecular goodness-of-fit: small-probe contact dots with explicit hydrogen atoms.
//!   https://doi.org/10.1006/jmbi.1998.2400
//!
//! ## TODO
//! - Consider migrating vdW/clash parameters from `ContactParams` into a dedicated
//!   `VdwParams` if contact/clash detection is decoupled from `physics::evaluate_contact`.

use crate::amino_acids::ElementExt;
use crate::model::AtomRecord;
use crate::physics::ContactParams;

/// Checks if two non-bonded heavy atoms violate van der Waals steric exclusion
/// (Word et al., 1999). Returns `Some(overlap_a)` when the overlap exceeds the tolerance.
#[inline]
pub fn check_steric_clash(
    atom_a: &AtomRecord,
    atom_b: &AtomRecord,
    dist_a: f32,
    p: &ContactParams,
) -> Option<f32> {
    let is_hbond_pair = atom_a.element.is_h_bond_partner() && atom_b.element.is_h_bond_partner();
    let tol = if is_hbond_pair {
        p.hbond_clash_tolerance_a
    } else {
        p.clash_tolerance_a
    };
    let overlap_a = (atom_a.vdw_radius_a + atom_b.vdw_radius_a - tol) - dist_a;
    if overlap_a > f32::EPSILON {
        Some(overlap_a)
    } else {
        None
    }
}
