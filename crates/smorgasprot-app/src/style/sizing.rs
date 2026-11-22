//! Atom radius encoding for the Interface tab.
//! Radii scale with camera zoom and never shrink below a legible minimum from [`AtomDrawParams`].

use crate::state::{AtomDrawParams, AtomSizeMode};
use crate::style::lenses::worst_clash_overlap;
use smorgasprot_core::AtomNode;

/// Screen radius (pixels) of an interface atom for the given sizing mode, zoom, and draw parameters.
pub fn atom_radius_px(node: &AtomNode, mode: AtomSizeMode, zoom: f32, params: &AtomDrawParams) -> f32 {
    let base = zoom * params.atom_radius_per_zoom;
    let scale = match mode {
        AtomSizeMode::Uniform => 1.15,
        // Grow with clash overlap so the worst offenders stand out.
        AtomSizeMode::ClashSeverity => 1.0 + worst_clash_overlap(node) * 2.2,
        // Area-proportional: radius ∝ sqrt(ΔSASA), floored for fully exposed atoms.
        AtomSizeMode::SqrtBuriedSasa => 0.65 + 0.25 * node.buried_sasa_a2.max(1.5).sqrt(),
    };
    (base * scale).max(params.min_atom_radius_px)
}
