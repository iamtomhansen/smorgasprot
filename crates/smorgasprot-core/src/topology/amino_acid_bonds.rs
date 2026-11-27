//! Static heavy-atom covalent bond templates for standard amino acid residues.
//!
//! ## Summary
//! Defines intra-residue covalent connectivity for the 20 standard amino acids plus common
//! selenized variants (`MSE`, `SEC`). The primary entry point is [`residue_bonds`], which maps a
//! 3-letter PDB/mmCIF residue name to a static slice of [`AaBond`] records.
//!
//! ## Considerations
//! This is a very simple implementation that only defines topology, and ignores all else about
//! amino acid chemistry (3D geometry, protonation, etc.).
//!
//! ## References
//! - Westbrook, J. D., Shao, C., Feng, Z. et al. (2015). The chemical component dictionary:
//!   complete descriptions of constituent molecules in experimentally determined 3D
//!   macromolecules in the Protein Data Bank. https://doi.org/10.1093/bioinformatics/btu789
//!
//! ## TODO
//! - Add common post-translational modifications (e.g., `SEP`, `TPO`, `PTR`, `CSO`) if needed.
//! - Handle explicit protonated states from e.g. NMR structures.

use crate::model::BondOrder;

/// A single intra-residue covalent bond in a chemical component template.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AaBond {
    pub atom_a: &'static str,
    pub atom_b: &'static str,
    pub order: BondOrder,
}

// Alias for AaBond constructor to make the entries more readable.
const fn bond(atom_a: &'static str, atom_b: &'static str, order: BondOrder) -> AaBond {
    AaBond { atom_a, atom_b, order }
}

// Aliases for bond orders to make the entries more readable.
const SINGLE: BondOrder = BondOrder::Single;
const DOUBLE: BondOrder = BondOrder::Double;
const AROMATIC: BondOrder = BondOrder::Aromatic;

/// Returns the static heavy-atom bond template for a standard 3-letter residue name, or `None`
/// for non-standard residues, ligands, and solvent.
pub fn residue_bonds(res_name: &str) -> Option<&'static [AaBond]> {
    match res_name.trim() {
        "ALA" => Some(ALA),
        "ARG" => Some(ARG),
        "ASN" => Some(ASN),
        "ASP" => Some(ASP),
        "CYS" => Some(CYS),
        "GLN" => Some(GLN),
        "GLU" => Some(GLU),
        "GLY" => Some(GLY),
        "HIS" => Some(HIS),
        "ILE" => Some(ILE),
        "LEU" => Some(LEU),
        "LYS" => Some(LYS),
        "MET" => Some(MET),
        "PHE" => Some(PHE),
        "PRO" => Some(PRO),
        "SER" => Some(SER),
        "THR" => Some(THR),
        "TRP" => Some(TRP),
        "TYR" => Some(TYR),
        "VAL" => Some(VAL),
        "MSE" => Some(MSE),
        "SEC" => Some(SEC),
        _ => None,
    }
}

const GLY: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
];

const ALA: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
];

const VAL: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG1", SINGLE),
    bond("CB", "CG2", SINGLE),
];

const LEU: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD1", SINGLE),
    bond("CG", "CD2", SINGLE),
];

const ILE: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG1", SINGLE),
    bond("CB", "CG2", SINGLE),
    bond("CG1", "CD1", SINGLE),
];

const PRO: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD", SINGLE),
    bond("CD", "N", SINGLE),
];

const SER: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "OG", SINGLE),
];

const THR: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "OG1", SINGLE),
    bond("CB", "CG2", SINGLE),
];

const CYS: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "SG", SINGLE),
];

const SEC: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "SE", SINGLE),
];

const MET: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "SD", SINGLE),
    bond("SD", "CE", SINGLE),
];

const MSE: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "SE", SINGLE),
    bond("SE", "CE", SINGLE),
];

const ASP: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "OD1", DOUBLE),
    bond("CG", "OD2", SINGLE),
];

const ASN: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "OD1", DOUBLE),
    bond("CG", "ND2", SINGLE),
];

const GLU: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD", SINGLE),
    bond("CD", "OE1", DOUBLE),
    bond("CD", "OE2", SINGLE),
];

const GLN: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD", SINGLE),
    bond("CD", "OE1", DOUBLE),
    bond("CD", "NE2", SINGLE),
];

const LYS: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD", SINGLE),
    bond("CD", "CE", SINGLE),
    bond("CE", "NZ", SINGLE),
];

const ARG: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD", SINGLE),
    bond("CD", "NE", SINGLE),
    bond("NE", "CZ", SINGLE),
    bond("CZ", "NH1", SINGLE),
    bond("CZ", "NH2", DOUBLE),
];

const HIS: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "ND1", AROMATIC),
    bond("CG", "CD2", AROMATIC),
    bond("ND1", "CE1", AROMATIC),
    bond("CE1", "NE2", AROMATIC),
    bond("CD2", "NE2", AROMATIC),
];

const PHE: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD1", AROMATIC),
    bond("CG", "CD2", AROMATIC),
    bond("CD1", "CE1", AROMATIC),
    bond("CD2", "CE2", AROMATIC),
    bond("CE1", "CZ", AROMATIC),
    bond("CE2", "CZ", AROMATIC),
];

const TYR: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD1", AROMATIC),
    bond("CG", "CD2", AROMATIC),
    bond("CD1", "CE1", AROMATIC),
    bond("CD2", "CE2", AROMATIC),
    bond("CE1", "CZ", AROMATIC),
    bond("CE2", "CZ", AROMATIC),
    bond("CZ", "OH", SINGLE),
];

const TRP: &[AaBond] = &[
    bond("N", "CA", SINGLE),
    bond("CA", "C", SINGLE),
    bond("C", "O", DOUBLE),
    bond("CA", "CB", SINGLE),
    bond("CB", "CG", SINGLE),
    bond("CG", "CD1", AROMATIC),
    bond("CG", "CD2", AROMATIC),
    bond("CD1", "NE1", AROMATIC),
    bond("NE1", "CE2", AROMATIC),
    bond("CD2", "CE2", AROMATIC),
    bond("CD2", "CE3", AROMATIC),
    bond("CE2", "CZ2", AROMATIC),
    bond("CE3", "CZ3", AROMATIC),
    bond("CZ2", "CH2", AROMATIC),
    bond("CZ3", "CH2", AROMATIC),
];
