//! Dedicated interface cartography, geometry, and contact manifold computation.
//!
//! ## Summary
//! Orchestrates 2D interface mapping and 3D interface geometry construction via
//! [`build_interface_geometry`] (configured by [`GeometryParams`]), assembling the bisector sheet,
//! contact envelope + Lawrence & Colman (1993) shape complementarity, visual negative-space voids,
//! and probe-accessible cavities.

pub mod cavities;
pub mod contour;
pub mod definition;
pub mod envelope;
pub mod flatten;
pub mod model;
pub mod negative_mesh;
pub mod uv_mesh;

pub use cavities::*;
pub use contour::*;
pub use definition::*;
pub use envelope::*;
pub use flatten::*;
pub use model::*;
pub use negative_mesh::*;
pub use uv_mesh::*;
pub use crate::isosurface::{compute_mesh_volume_a3, mesh_volume_area_centroid};

use crate::model::{ComplexStructure, SurfaceMesh};

crate::param_group! {
    /// Derived interface void/envelope geometry parameters.
    pub struct GeometryParams("geometry", "Interface Geometry") {
        /// Largest gap between the two surfaces still counted as a void (Å).
        void_max_gap_a: f32 = 2.8, 0.5..=8.0, "Inter-surface max void gap", "Å";
        /// Smooth-union blend radius for the void field (Å).
        void_smooth_k_a: f32 = 0.55, 0.0..=2.0, "Void field smooth-union blend", "Å";
        /// Void grid spacing (Å); smaller is finer and slower.
        void_grid_step_a: f32 = 0.95, 0.3..=3.0, "Void grid voxel step", "Å";
        /// Margin around the interface for the void grid (Å).
        void_bounding_margin_a: f32 = 3.5, 0.5..=10.0, "Void bounding grid margin", "Å";
        /// Implicit hydrogen / thermal skin added to heavy-atom vdW radii for void geometry (Å).
        implicit_h_skin_a: f32 = 0.15, 0.0..=1.0, "Implicit H skin (vdW padding)", "Å";
    }
}

/// Builds the comprehensive 3D interface geometry representations:
/// 1. Smoothed inter-chain interface separator sheet mesh.
/// 2. 2-sided contact envelope mesh and Lawrence & Colman (1993) shape complementarity `S_c`.
/// 3. Negative-space sub-solvent packing-crevice mesh and volume (Å³).
/// 4. Analytical probe-accessible buried cavities and solvent channels.
pub fn build_interface_geometry(
    complex: &ComplexStructure,
    partition: &InterfacePartition,
    map: &InterfaceMap,
    chain_meshes: &[SurfaceMesh],
    p: &GeometryParams,
    sheet_p: &InterfaceMeshParams,
    cavity_p: &CavityParams,
) -> InterfaceGeometry {
    let sheet_mesh = build_interface_sheet_mesh(complex, &map.nodes, &map.plane, sheet_p);
    let envelope_mesh =
        extract_interface_envelope_mesh(chain_meshes, complex, partition, &map.nodes);
    let shape_complementarity_sc =
        compute_shape_complementarity(&envelope_mesh, complex, partition);
    let negative_mesh = build_interface_negative_mesh(complex, &map.nodes, p);
    let void_volume_a3 = compute_mesh_volume_a3(&negative_mesh);
    let cavities = build_interface_cavities(complex, &map.nodes, p, cavity_p);

    InterfaceGeometry {
        sheet_mesh,
        envelope_mesh,
        negative_mesh,
        void_volume_a3,
        shape_complementarity_sc,
        cavities,
    }
}
