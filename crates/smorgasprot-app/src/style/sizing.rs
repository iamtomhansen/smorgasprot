//! Atom sizing modes and screen dimension calculations.

use super::lenses::worst_clash_on_node;
use serde::{Deserialize, Serialize};
use smorgasprot_core::AtomNode;

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum AtomSizeMode {
    SqrtBuriedSasa,
    Uniform,
    ClashSeverity,
}

impl AtomSizeMode {
    pub const ALL: [Self; 3] = [
        Self::SqrtBuriedSasa,
        Self::Uniform,
        Self::ClashSeverity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SqrtBuriedSasa => "Buried Interface Area (sqrt ΔSASA)",
            Self::Uniform => "Uniform Size",
            Self::ClashSeverity => "Steric Clash Severity",
        }
    }
}

/// Computes the atom display radius in screen pixels based on active camera zoom and sizing mode.
pub fn get_atom_radius_px(node: &AtomNode, size_mode: AtomSizeMode, cam_zoom: f32) -> f32 {
    let base_scale = cam_zoom * 0.28;
    match size_mode {
        AtomSizeMode::Uniform => (base_scale * 1.15).max(3.5),
        AtomSizeMode::ClashSeverity => {
            let boost = 1.0 + worst_clash_on_node(node) * 2.2;
            (base_scale * boost).max(3.5)
        }
        AtomSizeMode::SqrtBuriedSasa => {
            let s = (node.buried_sasa_a2.max(1.5)).sqrt();
            (base_scale * (0.65 + 0.25 * s)).max(3.5)
        }
    }
}
