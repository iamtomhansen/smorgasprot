//! Conformation-dependent non-covalent molecular interactions.
//!
//! ## Summary
//! Groups non-bonded interaction detectors (hydrogen bonds, salt bridges, metal coordinations,
//! and van der Waals contacts) separately from true covalent [`crate::topology`].
//!
//! ## Considerations
//! Pairwise comparisons of non-bonded heavy atoms are considered "interactions". van der Waals
//! contacts are implicitly assumed in the case of non-bonded heavy atoms that are *not* sterically
//! clashing, which also means that steric clashess (and their magnitudes) are calculated here.

pub mod coordinations;
pub mod h_bonds;
pub mod salt_bridges;
pub mod vdw_contacts;
