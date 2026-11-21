//! Core domain data structures for 3D molecular complexes and two-sided flattened interface maps.

use crate::amino_acids::ResidueKind;
pub use pdbtbx::Element;
use serde::{Deserialize, Serialize};

/// Dense index into `ComplexStructure::atoms`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct GlobalAtomIdx(pub u32);

/// Dense index into `ComplexStructure::residues`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct GlobalResidueIdx(pub u32);

/// Dense index into `ComplexStructure::chains`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ChainIdx(pub u32);

/// Dense index into `InterfaceMap::nodes`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct AtomNodeIdx(pub u32);

/// Dense index into `InterfaceMap::territories`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct TerritoryIdx(pub u32);

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

/// Layer 1: Full 3D complex containing all loaded protein chains and the active SideA / SideB partition.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComplexStructure {
    /// User-facing name of the complex (e.g. "7JVB", "nanobody-spike receptor", "my_cool_proteins", etc.)
    pub name: String,
    pub group_a_name: String,
    pub group_b_name: String,
    pub group_a_chains: Vec<String>,
    pub group_b_chains: Vec<String>,
    pub chains: Vec<ChainRecord>,
    pub residues: Vec<ResidueRecord>,
    pub atoms: Vec<AtomRecord>,
}

impl ComplexStructure {
    /// Returns `InterfaceSide::SideA` if `chain_id` is assigned to Group A,
    /// `InterfaceSide::SideB` if assigned to Group B, or `InterfaceSide::None` if unassigned.
    pub fn chain_side(&self, chain_id: &str) -> InterfaceSide {
        if self.group_a_chains.iter().any(|c| c == chain_id) {
            InterfaceSide::SideA
        } else if self.group_b_chains.iter().any(|c| c == chain_id) {
            InterfaceSide::SideB
        } else {
            InterfaceSide::None
        }
    }
}

/// Metadata and contiguous array ranges for a single polypeptide chain in [`ComplexStructure`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainRecord {
    pub idx: ChainIdx,
    /// The chain's PDB-style name as a string (e.g. "A", "B", etc.)
    pub chain_id: String,
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
    pub chain_id: String,
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
    pub atom_name: [u8; 4],
    pub element: Element,
    pub vdw_radius_a: f32,
    pub pos_3d: [f32; 3],
    pub b_factor: f32,
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

/// 3D projection-plane Π = (origin, axis_u, axis_v, normal) for the interface. Flat, but 3D-oriented.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectionPlane {
    pub origin: [f32; 3],
    pub axis_u: [f32; 3],
    pub axis_v: [f32; 3],
    pub normal: [f32; 3],
    pub hinge_u: f32,
}

/// Layer 2: Extracted dual-side interface cartography (SideA + SideB patches).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterfaceMap {
    pub plane: ProjectionPlane,
    pub territories: Vec<ResidueTerritory>,
    pub nodes: Vec<AtomNode>,
    pub edges: Vec<InterfaceEdge>,
    /// Approximated 3D interface mid-surface sheet mesh separating SideA and SideB.
    pub interface_sheet_mesh: SurfaceMesh,
}

/// The bounded 2D/3D territory on the interface map corresponding to a single residue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueTerritory {
    pub idx: TerritoryIdx,
    pub global_residue: GlobalResidueIdx,
    pub role: InterfaceSide,
    pub chain_id: String,
    pub label_top: String,
    pub label_bottom: String,
    pub patch_atoms: Vec<AtomNodeIdx>,
    pub buried_sasa_a2: f32,
    pub has_unsatisfied_polar: bool,
    /// Relaxed 2D contour polygon vertices `(u, v)` on `ProjectionPlane` (at `flatten_t = 1.0`).
    pub contour_uv: Vec<[f32; 2]>,
    /// True 3D contour polygon vertices `(x, y, z)` draped over the residue's 3D atoms (at `flatten_t = 0.0`).
    pub contour_3d: Vec<[f32; 3]>,
    /// Pole of inaccessibility in 2D `(u, v)` space for label placement (at `flatten_t = 1.0`).
    pub label_center_uv: [f32; 2],
    /// Pole of inaccessibility in 3D `(x, y, z)` space for label placement (at `flatten_t = 0.0`).
    pub label_center_3d: [f32; 3],
    pub local_uv_anchor: [f32; 2],
}

/// A single atom node in the interface map (serves both 2D and 3D spaces).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AtomNode {
    pub idx: AtomNodeIdx,
    pub global_atom: GlobalAtomIdx,
    pub territory: TerritoryIdx,
    pub role: InterfaceSide,
    /// Crystallographic isotropic B-factor (temperature factor, in A^2) or AlphaFold pLDDT (0-100).
    pub b_factor: f32,
    pub free_sasa_a2: f32,
    pub complex_sasa_a2: f32,
    pub buried_sasa_a2: f32,
    pub aliphatic_score: f32,
    pub pos_3d: [f32; 3],
    pub pos_uv_raw: [f32; 2],
    pub pos_uv: [f32; 2],
    pub is_unsatisfied_buried_polar: bool,
    pub interactions: Vec<AtomInteraction>,
}

/// Unified classification of an edge or contact between two interfacial atoms (intra-side or cross-interface).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Intra-residue covalent / first-shell geometry spring.
    SameResidue,
    /// General first-shell spatial neighbor / apolar van der Waals contact (<= 4.5 A).
    VanDerWaalsContact,
    /// Steric van der Waals clash (overlap_a > 0.05 A).
    Clash,
    /// Hydrogen bond (2.35..=3.50 A between polar N/O heavy atoms).
    HBond,
    /// Salt bridge (<= 4.00 A between oppositely charged sidechain groups).
    SaltBridge,
}

/// A single interaction record stored directly on a [`AtomNode`].
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq)]
pub struct AtomInteraction {
    pub partner: AtomNodeIdx,
    pub is_cross_interface: bool,
    pub kind: EdgeKind,
    pub dist_3d_a: f32,
    pub overlap_a: f32,
}

/// A unified non-directional edge between two [`AtomNode`]s in [`InterfaceMap::edges`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterfaceEdge {
    /// Node index of the first atom in the edge (arbitrarily chosen).
    pub a: AtomNodeIdx,
    /// Node index of the second atom in the edge (arbitrarily chosen).
    pub b: AtomNodeIdx,
    /// If the edge connects atoms from different sides of the interface.
    pub is_cross_interface: bool,
    /// Classification of the edge type (covalent, van der Waals, hydrogen bond, etc.).
    pub kind: EdgeKind,
    /// True 3D Euclidean distance between atoms `a` and `b` (at `flatten_t = 0.0`).
    pub dist_3d_a: f32,
    /// Relaxed 2D Euclidean distance in `(u, v)` space on `ProjectionPlane` (at `flatten_t = 1.0`).
    pub dist_2d_a: f32,
    /// Signed 2D metric distortion (`dist_2d_a - dist_3d_a`: positive = stretched, negative = compressed).
    pub strain_2d_a: f32,
    /// Calculated steric overlap between atoms `a` and `b`.
    pub overlap_a: f32,
}
