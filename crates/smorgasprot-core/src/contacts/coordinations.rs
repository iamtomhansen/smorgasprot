//! Metal-ion coordination geometry detection.
//!
//! ## Summary
//! Placeholder for identifying non-covalent metal–ligand coordination bonds (e.g., `Zn²⁺`,
//! `Mg²⁺`, `Ca²⁺`, `Fe²⁺/³⁺`, `Cu²⁺` coordinated by `CYS SG`, `HIS ND1/NE2`, `ASP OD1/OD2`,
//! `GLU OE1/OE2`, backbone carbonyls, and water).
//!
//! ## Considerations
//! Metal ion coordination is a surprisingly throny problem...
//! - **Chemical complexity**: Metal identity and oxidation state, expected coordination number
//!   (tetrahedral, square-planar, octahedral), element-specific metal–donor distance cutoffs, and
//!   electronic effects such as Jahn–Teller distortion (e.g., elongated axial ligands in `Cu²⁺`).
//! - **Current fallback**: Metal–carboxylate contacts are temporarily caught by
//!   [`crate::interactions::salt_bridges::is_salt_bridge`] via `element.is_metal_ion()`, which
//!   should be routed here once coordination detection is implemented.
//!
//! ## References
//! - Harding, M. M. (2001). Geometry of metal–ligand interactions in proteins.
//!   https://doi.org/10.1107/S0907444900019168
//!
//! ## TODO
//! - Implement metal–donor distance tables and coordination-sphere geometry checks (e.g.,
//!   tetrahedral `Zn²⁺`, octahedral `Mg²⁺`/`Ca²⁺`).
