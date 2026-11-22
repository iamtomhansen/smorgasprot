//! Dedicated interface cartography, geometry, and contact manifold computation.
//! This is the main purpose of the `smorgasprot-core` crate, which is designed for interface
//! analysis.

pub mod contour;
pub mod flatten;
pub mod geometry;
pub mod model;

pub use contour::*;
pub use flatten::*;
pub use geometry::*;
pub use model::*;
