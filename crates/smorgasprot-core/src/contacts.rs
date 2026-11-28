//! Unified molecular contacts: invariant covalent topology and distance/geometry-dependent
//! noncovalent interactions.
//!
//! ## Summary
//! Organizes all atom–atom connectivity in `smorgasprot-core` into two physical regimes:
//! - **Covalent topology** ([`amino_acid_bonds`], [`covalent`], [`ss_bonds`], [`isopeptides`]):
//!   fixed chemical bonds detected at structure load time.
//! - **Noncovalent interactions** ([`h_bonds`], [`salt_bridges`], [`vdw_contacts`], [`coordinations`]):
//!   spatial/electrostatic contacts evaluated across conformations and interfaces.

pub mod amino_acid_bonds;
pub mod coordinations;
pub mod covalent;
pub mod h_bonds;
pub mod isopeptides;
pub mod salt_bridges;
pub mod ss_bonds;
pub mod vdw_contacts;

pub use covalent::{BondParams, detect_bonds};
pub use h_bonds::is_hydrogen_bond_baker_hubbard;
pub use salt_bridges::is_salt_bridge;
pub use vdw_contacts::check_steric_clash;
