//! Display defaults for a freshly loaded dataset, as a pure function of its size.
//!
//! Deliberately has no dependency on `UiState`, `app.rs`, `jobs.rs`, or `loader.rs` — it only
//! ever answers "given this many atoms, what's a sensible starting presentation," never "what
//! should the app do." `app.rs` is a caller, not the decision-maker: it applies these defaults
//! when a dataset becomes active, but doesn't own the policy of what they are.

use crate::state::SurfaceCulling;
use smorgasprot_core::surface::GeodesicPreset;

/// Above this many heavy atoms, meshing every chain at the default preset stalls the first frame,
/// so new loads start at the coarsest preset instead (the user can raise it back from the panel).
const LARGE_COMPLEX_ATOM_THRESHOLD: usize = 15_000;

/// Starting presentation settings for a complex of `atom_count` heavy atoms.
pub struct DisplayDefaults {
    pub draw_preset: GeodesicPreset,
    pub culling: SurfaceCulling,
}

/// Initial tessellation preset and face culling for a complex of `atom_count` heavy atoms.
/// A large structure starts coarser and backface-culled, since the up-front mesh cost and the
/// per-frame translucency overdraw of a full-detail surface both scale with chain count.
pub fn defaults_for(atom_count: usize) -> DisplayDefaults {
    if atom_count > LARGE_COMPLEX_ATOM_THRESHOLD {
        DisplayDefaults { draw_preset: GeodesicPreset::Octahedron8, culling: SurfaceCulling::Backfaces }
    } else {
        DisplayDefaults { draw_preset: GeodesicPreset::Octahedron18, culling: SurfaceCulling::None }
    }
}
