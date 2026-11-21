//! Core molecular math (geometry, secondary structure, interatomic forces, etc.)
//! and presentation logic (2D projection, flattening, contouring).

pub mod amino_acids;
pub mod contour;
pub mod dssp;
pub mod flatten;
pub mod ingest;
pub mod interface_model;
pub mod minimize;
pub mod model;
pub mod physics;
pub mod rotamers;
pub mod sasa;
pub mod spatial;
pub mod surface;

pub use amino_acids::*;
pub use model::*;
