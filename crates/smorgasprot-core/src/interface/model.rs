//! Domain model for macromolecular interfaces and cartographic maps.

use super::definition::InterfaceSide;
use crate::model::{ChainName, GlobalAtomIdx, GlobalResidueIdx, SurfaceMesh};
use serde::{Deserialize, Serialize};

/// Dense index into `InterfaceMap::nodes`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct AtomNodeIdx(pub u32);

/// Dense index into `InterfaceMap::territories`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct TerritoryIdx(pub u32);

/// 3D projection-plane Π = (origin, axis_u, axis_v, normal) for the interface. Flat, but 3D-oriented.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectionPlane {
    pub origin: [f32; 3],
    pub axis_u: [f32; 3],
    pub axis_v: [f32; 3],
    pub normal: [f32; 3],
    pub hinge_u: f32,
}

/// Extracted dual-side interface cartography (SideA + SideB patches).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterfaceMap {
    pub plane: ProjectionPlane,
    pub territories: Vec<ResidueTerritory>,
    pub nodes: Vec<AtomNode>,
    pub edges: Vec<InterfaceEdge>,
}

/// Derived 3D interface geometric and visual representations (sheet, envelope, and negative voids).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct InterfaceGeometry {
    /// Approximated 3D interface mid-surface sheet mesh separating SideA and SideB.
    pub sheet_mesh: SurfaceMesh,
    /// 3D two-sided interface contact envelope mesh (duplicated in-place from interacting surfaces).
    pub envelope_mesh: SurfaceMesh,
    /// 3D negative-space interface void / sub-solvent cavity mesh.
    pub negative_mesh: SurfaceMesh,
    /// Total enclosed physical volume of the sub-solvent interfacial voids in cubic Angstroms (Å³).
    pub void_volume_a3: f32,
}

/// The bounded 2D/3D territory on the interface map corresponding to a single residue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueTerritory {
    pub idx: TerritoryIdx,
    pub global_residue: GlobalResidueIdx,
    pub role: InterfaceSide,
    pub chain_id: ChainName,
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
