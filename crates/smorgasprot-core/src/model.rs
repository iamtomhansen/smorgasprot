//! Core domain data structures for 3D molecular complexes.

use crate::amino_acids::ResidueKind;
pub use pdbtbx::Element;
use serde::{Deserialize, Serialize};

/// Dense index into `ComplexStructure::atoms`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct GlobalAtomIdx(pub u32);

/// Dense index into `ComplexStructure::residues`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct GlobalResidueIdx(pub u32);

/// Dense index into `ComplexStructure::chains`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct GlobalChainIdx(pub u32);

/// Secondary structure classification for residue labels and rendering.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SecStruct {
    Helix,
    Sheet,
    Loop,
}

/// Strongly typed PDB/mmCIF chain identifier (e.g. "A", "B", "H", "L").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[serde(transparent)]
pub struct ChainName(pub String);

impl ChainName {
    #[inline]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for ChainName {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for ChainName {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ChainName {
    #[inline]
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for ChainName {
    #[inline]
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl PartialEq<str> for ChainName {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ChainName {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for ChainName {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl std::borrow::Borrow<str> for ChainName {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ChainName {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Full 3D complex containing all loaded protein chains, residues, and atoms.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComplexStructure {
    /// User-facing name of the complex (e.g. "7JVB", "nanobody-spike receptor", etc.)
    pub name: String,
    pub chains: Vec<ChainRecord>,
    pub residues: Vec<ResidueRecord>,
    pub atoms: Vec<AtomRecord>,
    /// Covalent bond graph, detected once at ingest time (see `bonds::detect_bonds`).
    pub bonds: Vec<Bond>,
}

/// Classification of a covalent bond between two heavy atoms.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BondKind {
    /// N/CA/C/O(/OXT)-to-N/CA/C/O(/OXT): the invariant polypeptide backbone, whether within one
    /// residue or the inter-residue peptide link (C(i)-N(i+1), same chain).
    Backbone,
    /// Any other intra-residue bond (CB-CG, CB-OG1, etc.).
    Sidechain,
    /// Sulfur-sulfur disulfide bridge (typically two cysteines, same or different chains).
    Disulfide,
    /// Any other covalently bonded pair (glycan linkages, ligand attachment, modified-residues, etc.).
    Other,
}

/// Covalent bond multiplicity / aromaticity (ready for future bond-order & ring perception).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum BondOrder {
    #[default]
    Single,
    Double,
    Triple,
    Aromatic,
}

/// A single covalent bond between two heavy atoms.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Bond {
    pub a: GlobalAtomIdx,
    pub b: GlobalAtomIdx,
    pub kind: BondKind,
    #[serde(default)]
    pub order: BondOrder,
}

impl ComplexStructure {
    /// Evaluates an [`AtomSelection`] query across all atoms, returning matching indices.
    #[inline]
    pub fn select(&self, sel: &crate::selection::AtomSelection) -> Vec<GlobalAtomIdx> {
        sel.select_indices(self)
    }

    /// Evaluates an [`AtomSelection`] query across all atoms, returning a boolean mask.
    #[inline]
    pub fn select_mask(&self, sel: &crate::selection::AtomSelection) -> Vec<bool> {
        sel.select_mask(self)
    }
}

/// Metadata and contiguous array ranges for a single polypeptide chain in [`ComplexStructure`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainRecord {
    pub idx: GlobalChainIdx,
    /// The chain's PDB-style name as a string (e.g. "A", "B", etc.)
    pub chain_id: ChainName,
    pub residue_range: (u32, u32),
    pub atom_range: (u32, u32),
}

/// Data for a single residue (or coordinating ion / ligand) in a [`ComplexStructure`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueRecord {
    pub idx: GlobalResidueIdx,
    pub chain_idx: GlobalChainIdx,
    pub kind: ResidueKind,
    pub chain_id: ChainName,
    pub seq_num: i32,
    pub ins_code: char,
    pub res_name: [u8; 3],
    pub sec_struct: SecStruct,
    pub backbone_phi_psi: Option<(f32, f32)>,
    pub atom_range: (u32, u32),
}

/// Data for a single atom in a [`ComplexStructure`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AtomRecord {
    /// Dense index into `ComplexStructure::atoms` (0-based).
    pub idx: GlobalAtomIdx,
    /// Dense index into `ComplexStructure::residues` (0-based).
    pub residue_idx: GlobalResidueIdx,
    /// Dense index into `ComplexStructure::chains` (0-based).
    pub chain_idx: GlobalChainIdx,
    /// Atom serial number from the source PDB/mmCIF file (1-based).
    pub atom_serial: usize,
    pub atom_name: [u8; 4],
    pub element: Element,
    pub vdw_radius_a: f32,
    pub pos_3d: [f32; 3],
    /// B-factor (temperature factor) for the atom.
    pub b_factor: f32,
    pub occupancy: f32,
}

/// 3D triangle mesh for a chain's molecular surface.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SurfaceMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Nearest heavy atom index for each surface vertex (enables O(1) live vertex highlighting).
    pub nearest_atom: Vec<GlobalAtomIdx>,
    /// Triangle index triplets `(i0, i1, i2)` into `vertices`.
    pub triangles: Vec<[u32; 3]>,
}
