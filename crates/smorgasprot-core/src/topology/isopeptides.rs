//! Isopeptide bond detection (e.g., Lys–Asp/Glu/Asn/Gln crosslinks and ubiquitin-like C-term links).
//!
//! ## Summary
//! Placeholder for detecting non-canonical amide (`C(=O)–N`) linkages outside the main
//! polypeptide backbone:
//! - **Sidechain–sidechain crosslinks**: `LYS NZ` linked to `ASP CG`, `GLU CD`, `ASN CG`, or
//!   `GLN CD` (e.g., autocatalytic intramolecular isopeptide bonds in bacterial pili/adhesins).
//! - **Terminal–sidechain branching**: C-terminal `C` linked to `LYS NZ` (ubiquitin/SUMOylation),
//!   or N-terminal `N` linked to `ASP CG` / `GLU CD`.
//!
//! ## Considerations
//! - **Distinguishing from salt bridges**: A true covalent amide `C–N` bond is ~1.33 Å
//!   (typically 1.15–1.65 Å), whereas `LYS NZ`···`ASP CG`/`GLU CD` in a salt bridge is ≥ 3.2 Å.
//! - **Explicit annotations**: Once `pdbtbx` supports `CONECT` / `struct_conn` (`covale`)
//!   records, explicit crosslink annotations should take precedence over geometric perception.
//! - **Planarity**: Optional validation that the carbonyl carbon (`C`/`CG`/`CD`) and `NZ`/`N`
//!   environment is trigonal planar ($sp^2$).
//!
//! ## References
//! - Kang, H. J., Coulibaly, F., Clow, F. et al. (2007). Stabilizing isopeptide bonds revealed in
//!   Gram-positive bacterial pilus structure. https://doi.org/10.1126/science.1145806
//!
//! ## TODO
//! - Implement `IsopeptideParams` and `detect_isopeptides` (distance + $sp^2$ planarity check)
//!   and integrate with `covalent::detect_bonds`.
