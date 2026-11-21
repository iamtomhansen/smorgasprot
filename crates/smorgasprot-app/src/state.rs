//! Application session state, workspace tabs, and active display modes.

use serde::{Deserialize, Serialize};

pub use crate::camera::{CameraState, compute_interface_atom_local_3d, get_explode_offset_local, world_to_plane_local};
pub use crate::style::{AtomSizeMode, BFactorMode, ColorLens, get_atom_fill_color, get_atom_radius_px, has_interaction_kind, worst_clash_on_node};

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    Complex,
    Interface,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum InterfaceDisplayMode {
    SmoothedSdf,
    ContactEnvelope,
    NegativeSpace,
    Off,
}

impl InterfaceDisplayMode {
    pub const ALL: [Self; 4] = [
        Self::SmoothedSdf,
        Self::ContactEnvelope,
        Self::NegativeSpace,
        Self::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SmoothedSdf => "Smoothed SDF Sheet",
            Self::ContactEnvelope => "2-Sided Contact Envelope",
            Self::NegativeSpace => "Negative Space (Sub-Solvent Voids)",
            Self::Off => "Off",
        }
    }
}
