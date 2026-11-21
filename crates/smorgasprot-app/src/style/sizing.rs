//! Atom radius encoding for the Interface tab.
//! Radii scale with camera zoom and never shrink below a legible minimum.

use crate::state::AtomSizeMode;
use crate::style::lenses::worst_clash_overlap;
use smorgasprot_core::AtomNode;

/// Smallest drawn atom radius in pixels.
const MIN_RADIUS_PX: f32 = 3.5;
/// Base radius as a fraction of zoom (pixels per Å).
const BASE_RADIUS_PER_ZOOM: f32 = 0.28;

/// Screen radius (pixels) of an interface atom for the given sizing mode and zoom.
pub fn atom_radius_px(node: &AtomNode, mode: AtomSizeMode, zoom: f32) -> f32 {
    let base = zoom * BASE_RADIUS_PER_ZOOM;
    let scale = match mode {
        AtomSizeMode::Uniform => 1.15,
        // Grow with clash overlap so the worst offenders stand out.
        AtomSizeMode::ClashSeverity => 1.0 + worst_clash_overlap(node) * 2.2,
        // Area-proportional: radius ∝ sqrt(ΔSASA), floored for fully exposed atoms.
        AtomSizeMode::SqrtBuriedSasa => 0.65 + 0.25 * node.buried_sasa_a2.max(1.5).sqrt(),
    };
    (base * scale).max(MIN_RADIUS_PX)
}
