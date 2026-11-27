//! Hydrogen bond detection using Baker–Hubbard / McDonald–Thornton heavy-atom criteria.
//!
//! ## Summary
//! Evaluates whether two polar heavy atoms (`N`, `O`) form a hydrogen bond based on
//! donor–acceptor distance and antecedent covalent bond angles. The primary entry point is
//! [`is_hydrogen_bond_baker_hubbard`].
//!
//! ## Considerations
//! - **Heavy-atom representation**: Because `smorgasprot-core` omits explicit hydrogens during
//!   standard ingestion, hydrogen-bond geometry is inferred from heavy-atom donor/acceptor pairs
//!   and their covalently bonded antecedent atoms (`base_a`, `base_b`).
//! - **Antecedent angle fallback**: When either `base_a` or `base_b` is `None` (e.g. isolated
//!   water or missing covalent neighbor), that endpoint's angular check is skipped.
//! - **Parameters**: Currently reads thresholds from [`ContactParams`] in `physics.rs`.
//!
//! ## References
//! - Baker, E. N., Hubbard, R. E. (1984). Hydrogen bonding in globular proteins.
//!   https://doi.org/10.1016/0079-6107(84)90007-5
//! - McDonald, I. K., Thornton, J. M. (1994). Satisfying hydrogen bonding potential in proteins.
//!   https://doi.org/10.1006/jmbi.1994.1334

use glam::Vec3;

use crate::amino_acids::ElementExt;
use crate::model::AtomRecord;
use crate::physics::ContactParams;

/// Baker & Hubbard (1984) / McDonald & Thornton (1994) heavy-atom hydrogen bond detector.
///
/// Requires:
/// 1. Both atoms are polar N/O heavy atoms (`ElementExt::is_h_bond_partner`),
/// 2. Donor–acceptor distance `hbond_min_dist_a <= dist_a <= hbond_max_dist_a` (2.35–3.50 Å), and
/// 3. Antecedent covalent bond angles `angle(BaseA–A···B) >= 100°` and
///    `angle(A···B–BaseB) >= 100°` (when covalent base atoms are present).
pub fn is_hydrogen_bond_baker_hubbard(
    atom_a: &AtomRecord,
    base_a: Option<Vec3>,
    atom_b: &AtomRecord,
    base_b: Option<Vec3>,
    dist_a: f32,
    p: &ContactParams,
) -> bool {
    // 1. Filter to polar heavy-atom partners within the donor–acceptor distance window.
    if !atom_a.element.is_h_bond_partner()
        || !atom_b.element.is_h_bond_partner()
        || !(p.hbond_min_dist_a..=p.hbond_max_dist_a).contains(&dist_a)
    {
        return false;
    }

    let pa = Vec3::from(atom_a.pos_3d);
    let pb = Vec3::from(atom_b.pos_3d);
    // cos(100°) ≈ -0.173648
    let max_cos = (p.hbond_min_antecedent_angle_deg.to_radians()).cos();

    // 2. Check antecedent angle at atom A: obtuse (>= 100°) iff dot(u_ba, u_ab) <= cos(100°).
    if let Some(ba) = base_a {
        let u_ba = (ba - pa).normalize_or_zero();
        let u_ab = (pb - pa).normalize_or_zero();
        if u_ba.dot(u_ab) > max_cos {
            return false;
        }
    }

    // 3. Check antecedent angle at atom B.
    if let Some(bb) = base_b {
        let u_bb = (bb - pb).normalize_or_zero();
        let u_ba = (pa - pb).normalize_or_zero();
        if u_bb.dot(u_ba) > max_cos {
            return false;
        }
    }

    true
}
