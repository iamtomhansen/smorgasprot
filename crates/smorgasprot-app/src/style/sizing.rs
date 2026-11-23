//! Scalar visual-channel encodings: node radii, node border widths, edge widths, and territory border widths.
//! Normalizes physical metrics into `[0, 1]` and projects them onto screen-space pixel dimensions.

use crate::state::{AtomDrawParams, AtomSizeMode, BFactorMode, EdgeWidthLens, LensParams, TerritoryStrokeLens};
use crate::style::lenses::{contact_width, worst_clash_overlap};
use smorgasprot_core::{AtomNode, InterfaceEdge, ResidueTerritory};

/// Normalized mobility in `[0, 1]` (`0` = rigid / high pLDDT confidence, `1` = labile / low confidence).
pub fn node_mobility_t(node: &AtomNode, bfactor_mode: BFactorMode, params: &LensParams) -> f32 {
    let (v0, v1) = match bfactor_mode {
        BFactorMode::AlphaFold => (params.plddt_very_high_min, params.plddt_low_min),
        BFactorMode::Crystallographic => (params.bfactor_rigid_max_a2, params.bfactor_flexible_max_a2),
    };
    let span = v1 - v0;
    if span.abs() < 1e-6 { 0.5 } else { ((node.b_factor - v0) / span).clamp(0.0, 1.0) }
}

/// Normalized `[0, 1]` scalar projection for a node metric.
pub fn node_scalar_t(node: &AtomNode, mode: AtomSizeMode, bfactor_mode: BFactorMode, params: &LensParams) -> f32 {
    match mode {
        AtomSizeMode::Fixed => 0.5,
        AtomSizeMode::Off => 0.0,
        AtomSizeMode::SqrtBuriedSasa => (node.buried_sasa_a2.max(0.0).sqrt() / 5.0).clamp(0.0, 1.0),
        AtomSizeMode::ClashSeverity => (worst_clash_overlap(node) / 0.8).clamp(0.0, 1.0),
        AtomSizeMode::Mobility => node_mobility_t(node, bfactor_mode, params),
        AtomSizeMode::Rigidity => 1.0 - node_mobility_t(node, bfactor_mode, params),
        AtomSizeMode::AliphaticScore => node.aliphatic_score.clamp(0.0, 1.0),
    }
}

/// Screen radius (pixels) of an interface atom for the active size channel, zoom, and parameters.
pub fn atom_radius_px(
    node: &AtomNode,
    mode: AtomSizeMode,
    fixed_scale: f32,
    bfactor_mode: BFactorMode,
    zoom: f32,
    draw: &AtomDrawParams,
    lenses: &LensParams,
) -> f32 {
    if mode == AtomSizeMode::Off {
        return 0.0;
    }
    let base = zoom * draw.atom_radius_per_zoom;
    let scale = match mode {
        AtomSizeMode::Fixed => fixed_scale,
        AtomSizeMode::Off => 0.0,
        AtomSizeMode::ClashSeverity => 1.0 + worst_clash_overlap(node) * 2.2,
        AtomSizeMode::SqrtBuriedSasa => 0.65 + 0.25 * node.buried_sasa_a2.max(1.5).sqrt(),
        AtomSizeMode::Rigidity | AtomSizeMode::Mobility | AtomSizeMode::AliphaticScore => {
            0.70 + 0.95 * node_scalar_t(node, mode, bfactor_mode, lenses)
        }
    };
    (base * scale).max(draw.min_atom_radius_px)
}

/// Border stroke width (pixels) of an interface atom node.
pub fn atom_stroke_width_px(
    node: &AtomNode,
    mode: AtomSizeMode,
    fixed_width_px: f32,
    bfactor_mode: BFactorMode,
    hovered: bool,
    lenses: &LensParams,
) -> f32 {
    if hovered {
        return 2.5;
    }
    match mode {
        AtomSizeMode::Off => 0.0,
        AtomSizeMode::Fixed => fixed_width_px,
        _ => 0.8 + 2.2 * node_scalar_t(node, mode, bfactor_mode, lenses),
    }
}

/// Stroke width (pixels) of a contact edge under the active [`EdgeWidthLens`].
pub fn edge_width_px(
    edge: &InterfaceEdge,
    nodes: &[AtomNode],
    lens: EdgeWidthLens,
    fixed_width_px: f32,
    bfactor_mode: BFactorMode,
    params: &LensParams,
) -> f32 {
    match lens {
        EdgeWidthLens::Off => 0.0,
        EdgeWidthLens::Fixed => fixed_width_px,
        EdgeWidthLens::InteractionKind => contact_width(edge.kind),
        EdgeWidthLens::EndpointRigidity => {
            let ra = nodes.get(edge.a.0 as usize).map_or(0.5, |n| 1.0 - node_mobility_t(n, bfactor_mode, params));
            let rb = nodes.get(edge.b.0 as usize).map_or(0.5, |n| 1.0 - node_mobility_t(n, bfactor_mode, params));
            0.8 + 2.4 * (0.5 * (ra + rb))
        }
        EdgeWidthLens::Proximity3D => {
            let t = ((4.5 - edge.dist_3d_a) / (4.5 - 2.2)).clamp(0.0, 1.0);
            0.8 + 2.4 * t
        }
        EdgeWidthLens::ClashSeverity => {
            let t = (edge.overlap_a / 0.6).clamp(0.0, 1.0);
            0.9 + 2.6 * t
        }
        EdgeWidthLens::Strain2D => {
            let t = (edge.strain_2d_a.abs() / 1.5).clamp(0.0, 1.0);
            0.8 + 2.4 * t
        }
    }
}

/// Border stroke width (pixels) of a [`ResidueTerritory`] contour polygon.
pub fn territory_stroke_width_px(
    terr: &ResidueTerritory,
    nodes: &[AtomNode],
    lens: TerritoryStrokeLens,
    fixed_width_px: f32,
    bfactor_mode: BFactorMode,
    selected: bool,
    params: &LensParams,
) -> f32 {
    if lens == TerritoryStrokeLens::Off {
        return if selected { 1.4 } else { 0.0 };
    }
    let base = match lens {
        TerritoryStrokeLens::Off => 0.0,
        TerritoryStrokeLens::Fixed => fixed_width_px,
        TerritoryStrokeLens::BuriedSasa => 0.9 + 1.8 * (terr.buried_sasa_a2 / 80.0).clamp(0.0, 1.0),
        TerritoryStrokeLens::UnsatisfiedPolar => {
            if terr.has_unsatisfied_polar { 2.4 } else { 1.1 }
        }
        TerritoryStrokeLens::AvgRigidity => {
            if terr.patch_atoms.is_empty() {
                1.2
            } else {
                let sum: f32 = terr
                    .patch_atoms
                    .iter()
                    .filter_map(|idx| nodes.get(idx.0 as usize))
                    .map(|n| 1.0 - node_mobility_t(n, bfactor_mode, params))
                    .sum();
                let avg = sum / terr.patch_atoms.len() as f32;
                0.8 + 2.0 * avg
            }
        }
    };
    if selected { base + 0.8 } else { base }
}
