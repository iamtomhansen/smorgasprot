//! Core molecular math (geometry, secondary structure, interatomic forces, etc.)
//! and presentation logic (2D projection, flattening, contouring).

pub mod amino_acids;
pub mod dssp;
pub mod ingest;
pub mod interactions;
pub mod interface;
pub mod isosurface;
pub mod minimize;
pub mod model;
pub mod params;
pub mod physics;
pub mod rotamers;
pub mod sasa;
pub mod selection;
pub mod spatial;
pub mod surface;
pub mod topology;

pub use amino_acids::*;
pub use interface::*;
pub use model::*;
pub use params::AnalysisParams;
pub use selection::AtomSelection;

// Compatibility aliases for submodule paths.
pub use interface::contour;
pub use interface::flatten;
