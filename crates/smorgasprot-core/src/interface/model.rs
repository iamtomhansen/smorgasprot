//! Domain model for macromolecular interfaces and cartographic interface maps.
//!
//! ## Summary
//! Defines the core data structures for 2D/3D interface analysis: [`InterfaceMap`] (containing
//! [`ProjectionPlane`], [`ResidueTerritory`], [`AtomNode`], and [`InterfaceEdge`]) and
//! [`InterfaceGeometry`] (sheet, contact envelope, negative-space voids, and cavities).
//!
//! ## Considerations
//! - **Dense indexing**: [`AtomNodeIdx`] and [`TerritoryIdx`] index directly into
//!   [`InterfaceMap::nodes`] and [`InterfaceMap::territories`].
//! - **Units**: Coordinates and distances are in Å; SASA values are in Å²; volumes are in Å³.
//!   `strain_2d_a = dist_2d_a - dist_3d_a` (positive = stretched, negative = compressed).
//!
//! ## TODO
//! - Keep `EdgeKind` aligned as new interaction types (e.g. metal coordinations) are added.

use super::definition::InterfaceSide;
use crate::model::{ChainName, GlobalAtomIdx, GlobalResidueIdx, SurfaceMesh};
pub use crate::model::EdgeKind;
use serde::{Deserialize, Serialize};

const HINGE_OFFSET_A: f32 = 3.5; // Å

/// Dense index into `InterfaceMap::nodes`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct AtomNodeIdx(pub u32);

/// Dense index into `InterfaceMap::territories`.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct TerritoryIdx(pub u32);

/// 3D projection-plane Π = (origin, axis_u, axis_v, normal) for the interface.
/// Flat, but 3D-oriented.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectionPlane {
    /// Origin of the 2D projection plane in 3D space (center of the plane).
    pub origin: [f32; 3],
    /// Long axis of the 2D projection plane (u-axis) in 3D space.
    pub axis_u: [f32; 3],
    /// Short axis of the 2D projection plane (v-axis) in 3D space.
    pub axis_v: [f32; 3],
    /// Normal vector of the 2D projection plane in 3D space.
    pub normal: [f32; 3],
}

/// Extracted dual-side interface cartography (SideA + SideB patches).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterfaceMap {
    /// The 3D projection plane Π = (origin, axis_u, axis_v, normal) for the interface.
    pub plane: ProjectionPlane,
    /// The territories on the map corresponding to each residue in the interface.
    pub territories: Vec<ResidueTerritory>,
    /// The individual atom nodes on the map corresponding to each atom in the interface.
    pub nodes: Vec<AtomNode>,
    /// Every unique edge between two atoms in the interface (intra-side or cross-interface).
    pub edges: Vec<InterfaceEdge>,
}

impl InterfaceMap {
    /// Derived `u`-coordinate of the open-book butterfly hinge (`min_u - 3.5 Å`).
    pub fn hinge_u(&self) -> f32 {
        let min_u = self.nodes.iter().map(|n| n.pos_uv[0]).fold(f32::INFINITY, f32::min);
        if min_u.is_finite() { min_u - HINGE_OFFSET_A } else { -HINGE_OFFSET_A }
    }
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
    /// Lawrence & Colman (1993) shape complementarity S_c ∈ [-1, 1] on the contact envelope.
    pub shape_complementarity_sc: f32,
    /// Probe-accessible buried pockets and solvent-connected channels.
    pub cavities: super::cavities::CavityAnalysis,
}

/// The bounded 2D/3D territory on the interface map corresponding to a single residue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueTerritory {
    /// The dense index of this residue territory in the `InterfaceMap`.
    pub idx: TerritoryIdx,
    /// The global residue index of the residue in the `ComplexStructure`.
    pub global_residue: GlobalResidueIdx,
    /// The side of the interface to which this residue belongs.
    pub side: InterfaceSide,
    /// The chain to which this residue belongs.
    pub chain_id: ChainName,
    /// First line of the label for this territory.
    pub label_top: String,
    /// Second line of the label for this territory.
    pub label_bottom: String,
    /// Indices of the [`AtomNode`]s that belong to this residue territory.
    pub member_atoms: Vec<AtomNodeIdx>,
    /// The total buried solvent-accessible surface area (SASA) of this residue in Å².
    pub buried_sasa_a2: f32,
    /// Total buried SASA (Å²) of polar atoms in this residue with no H-bonds or salt bridges.
    pub unsatisfied_polar_buried_sasa_a2: f32,
    /// 2D contour boundary polygon vertices `(u, v)` on `ProjectionPlane` (at `flatten_t = 1.0`).
    pub contour_uv: Vec<[f32; 2]>,
    /// 3D boundary polygon vertices draped through the residue's atomic centers (blend = 0.0).
    pub contour_3d: Vec<[f32; 3]>,
    /// 3D boundary polygon vertices draped onto the curved `uv_mesh` interface sheet (blend = 1.0).
    #[serde(default)]
    pub contour_3d_sheet: Vec<[f32; 3]>,
    /// Filled territory mesh 2D `(u, v)` vertices (boundary `0..contour_uv.len()` + interior Steiner points).
    #[serde(default)]
    pub contour_mesh_uv: Vec<[f32; 2]>,
    /// Filled territory mesh 3D vertices draped through atomic centers (1:1 with `contour_mesh_uv`).
    #[serde(default)]
    pub contour_mesh_3d_atoms: Vec<[f32; 3]>,
    /// Filled territory mesh 3D vertices draped onto `uv_mesh` (1:1 with `contour_mesh_uv`).
    #[serde(default)]
    pub contour_mesh_3d_sheet: Vec<[f32; 3]>,
    /// Triangulation indexing into `contour_mesh_*` for rendering the concave territory fill.
    pub contour_triangles: Vec<[u32; 3]>,
    /// Pole of inaccessibility in 2D `(u, v)` space for label placement (at `flatten_t = 1.0`).
    pub label_center_uv: [f32; 2],
    /// Pole of inaccessibility in 3D `(x, y, z)` space draped to atoms (blend = 0.0).
    pub label_center_3d: [f32; 3],
    /// Pole of inaccessibility in 3D `(x, y, z)` space draped to `uv_mesh` (blend = 1.0).
    #[serde(default)]
    pub label_center_3d_sheet: [f32; 3],
    pub local_uv_anchor: [f32; 2],
}

/// A single atom node in the interface map (serves both 2D and 3D spaces).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AtomNode {
    pub idx: AtomNodeIdx,
    pub global_atom: GlobalAtomIdx,
    pub territory: TerritoryIdx,
    pub side: InterfaceSide,
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
    /// All interactions (edges) between this atom and its neighbors, stored directly on the node for fast access.
    pub interactions: Vec<AtomInteraction>,
}

/// A single interaction record stored directly on a [`AtomNode`].
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq)]
pub struct AtomInteraction {
    pub partner: AtomNodeIdx,
    pub is_cross_interface: bool,
    pub kind: EdgeKind,
    pub dist_3d_a: f32,
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
}
