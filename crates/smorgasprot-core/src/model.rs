//! Core domain data structures for 3D molecular complexes and two-sided flattened interface maps.

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
pub struct ChainIdx(pub u32);

/// Which side of an arbitrarily defined interface a given chain, residue, or atom belongs to.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum InterfaceSide {
    SideA,
    SideB,
    None,
}

impl InterfaceSide {
    #[inline]
    pub const fn opposite(self) -> Self {
        match self {
            Self::SideA => Self::SideB,
            Self::SideB => Self::SideA,
            Self::None => Self::None,
        }
    }

    #[inline]
    pub const fn is_assigned(self) -> bool {
        !matches!(self, Self::None)
    }
}

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

/// Demarcation of an interface partition between two groups of chains.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InterfacePartition {
    pub name: String,
    pub group_a_name: String,
    pub group_b_name: String,
    pub group_a_chains: Vec<ChainName>,
    pub group_b_chains: Vec<ChainName>,
}

impl InterfacePartition {
    pub fn new(
        name: impl Into<String>,
        group_a_name: impl Into<String>,
        group_b_name: impl Into<String>,
        group_a_chains: Vec<ChainName>,
        group_b_chains: Vec<ChainName>,
    ) -> Self {
        Self {
            name: name.into(),
            group_a_name: group_a_name.into(),
            group_b_name: group_b_name.into(),
            group_a_chains,
            group_b_chains,
        }
    }

    /// Returns `InterfaceSide::SideA` if `chain_id` is assigned to Group A,
    /// `InterfaceSide::SideB` if assigned to Group B, or `InterfaceSide::None` if unassigned.
    pub fn chain_side(&self, chain_id: &str) -> InterfaceSide {
        if self.group_a_chains.iter().any(|c| c.as_str() == chain_id) {
            InterfaceSide::SideA
        } else if self.group_b_chains.iter().any(|c| c.as_str() == chain_id) {
            InterfaceSide::SideB
        } else {
            InterfaceSide::None
        }
    }
}

/// Layer 1: Full 3D complex containing all loaded protein chains and the active SideA / SideB partition.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComplexStructure {
    /// User-facing name of the complex (e.g. "7JVB", "nanobody-spike receptor", "my_cool_proteins", etc.)
    pub name: String,
    pub group_a_name: String,
    pub group_b_name: String,
    pub group_a_chains: Vec<ChainName>,
    pub group_b_chains: Vec<ChainName>,
    pub chains: Vec<ChainRecord>,
    pub residues: Vec<ResidueRecord>,
    pub atoms: Vec<AtomRecord>,
}

impl ComplexStructure {
    /// Returns `InterfaceSide::SideA` if `chain_id` is assigned to Group A,
    /// `InterfaceSide::SideB` if assigned to Group B, or `InterfaceSide::None` if unassigned.
    pub fn chain_side(&self, chain_id: &str) -> InterfaceSide {
        if self.group_a_chains.iter().any(|c| c.as_str() == chain_id) {
            InterfaceSide::SideA
        } else if self.group_b_chains.iter().any(|c| c.as_str() == chain_id) {
            InterfaceSide::SideB
        } else {
            InterfaceSide::None
        }
    }

    /// Returns the active interface partition for this complex.
    pub fn partition(&self) -> InterfacePartition {
        InterfacePartition {
            name: self.name.clone(),
            group_a_name: self.group_a_name.clone(),
            group_b_name: self.group_b_name.clone(),
            group_a_chains: self.group_a_chains.clone(),
            group_b_chains: self.group_b_chains.clone(),
        }
    }
}

/// Metadata and contiguous array ranges for a single polypeptide chain in [`ComplexStructure`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainRecord {
    pub idx: ChainIdx,
    /// The chain's PDB-style name as a string (e.g. "A", "B", etc.)
    pub chain_id: ChainName,
    pub residue_range: (u32, u32),
    pub atom_range: (u32, u32),
}

/// Data for a single residue (or coordinating ion / ligand) in a [`ComplexStructure`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueRecord {
    pub idx: GlobalResidueIdx,
    pub chain_idx: ChainIdx,
    pub role: InterfaceSide,
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
    pub idx: GlobalAtomIdx,
    pub residue_idx: GlobalResidueIdx,
    pub chain_idx: ChainIdx,
    pub role: InterfaceSide,
    pub atom_serial: usize,
    pub atom_name: [u8; 4],
    pub element: Element,
    pub vdw_radius_a: f32,
    pub pos_3d: [f32; 3],
    pub b_factor: f32,
    pub occupancy: f32,
}

/// 3D triangle mesh for a chain's molecular surface (or the inter-chain interface sheet in Cartoon mode).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SurfaceMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Nearest heavy atom index for each surface vertex (enables O(1) live interface vertex highlighting).
    pub nearest_atom: Vec<GlobalAtomIdx>,
    /// Triangle index triplets `(i0, i1, i2)` into `vertices`.
    pub triangles: Vec<[u32; 3]>,
}
