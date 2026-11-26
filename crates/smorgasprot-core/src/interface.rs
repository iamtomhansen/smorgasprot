//! Dedicated interface cartography, geometry, and contact manifold computation.
//! This is the main purpose of the `smorgasprot-core` crate, which is designed for interface
//! analysis.

pub mod cavities;
pub mod contour;
pub mod definition;
pub mod flatten;
pub mod geometry;
pub mod model;
pub mod uv_mesh;

pub use cavities::*;
pub use contour::*;
pub use definition::*;
pub use flatten::*;
pub use geometry::*;
pub use model::*;
pub use uv_mesh::*;
