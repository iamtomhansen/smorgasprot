//! Scalar visual-channel encodings: unified [`ScalarMetric`] evaluation, node radii, and border/edge widths.
//! Normalizes physical metrics into `[0, 1]` and projects them onto screen-space pixel dimensions.

use crate::state::{AtomDrawParams, AtomSizeMode, BFactorMode, EdgeWidthLens, LensParams, ScalarMetric, TerritoryStrokeLens};
use crate::style::lenses::{contact_width, worst_clash_overlap};
use crate::style::Palette;
use egui::Color32;
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

impl ScalarMetric {
    /// Raw un-normalized scalar value for an [`AtomNode`] (used for numeric labels and tooltips).
    pub fn atom_raw(self, node: &AtomNode) -> f32 {
        match self {
            Self::BuriedSasa => node.buried_sasa_a2,
            Self::Mobility | Self::Rigidity => node.b_factor,
            Self::AliphaticScore => node.aliphatic_score,
            Self::ClashSeverity => worst_clash_overlap(node),
        }
    }

    /// Normalized `[0, 1]` value for an [`AtomNode`] (used for colormaps and sizing).
    pub fn atom_norm(self, node: &AtomNode, bfactor_mode: BFactorMode, params: &LensParams) -> f32 {
        match self {
            Self::BuriedSasa => (node.buried_sasa_a2 / params.atom_buried_sasa_max_a2.max(1e-3)).clamp(0.0, 1.0),
            Self::Mobility => node_mobility_t(node, bfactor_mode, params),
            Self::Rigidity => 1.0 - node_mobility_t(node, bfactor_mode, params),
            Self::AliphaticScore => node.aliphatic_score.clamp(0.0, 1.0),
            Self::ClashSeverity => (worst_clash_overlap(node) / params.clash_severity_max_a.max(1e-3)).clamp(0.0, 1.0),
        }
    }

    /// Raw un-normalized scalar value for a [`ResidueTerritory`] (aggregated across its interfacial patch atoms).
    pub fn territory_raw(self, terr: &ResidueTerritory, nodes: &[AtomNode]) -> f32 {
        match self {
            Self::BuriedSasa => terr.buried_sasa_a2,
            Self::Mobility | Self::Rigidity => {
                if terr.patch_atoms.is_empty() {
                    0.0
                } else {
                    let sum: f32 = terr
                        .patch_atoms
                        .iter()
                        .filter_map(|idx| nodes.get(idx.0 as usize))
                        .map(|n| n.b_factor)
                        .sum();
                    sum / terr.patch_atoms.len() as f32
                }
            }
            Self::AliphaticScore => {
                if terr.patch_atoms.is_empty() {
                    0.0
                } else {
                    let sum: f32 = terr
                        .patch_atoms
                        .iter()
                        .filter_map(|idx| nodes.get(idx.0 as usize))
                        .map(|n| n.aliphatic_score)
                        .sum();
                    sum / terr.patch_atoms.len() as f32
                }
            }
            Self::ClashSeverity => terr
                .patch_atoms
                .iter()
                .filter_map(|idx| nodes.get(idx.0 as usize))
                .map(worst_clash_overlap)
                .fold(0.0, f32::max),
        }
    }

    /// Normalized `[0, 1]` value for a [`ResidueTerritory`] (used for territory colormaps and border widths).
    pub fn territory_norm(
        self,
        terr: &ResidueTerritory,
        nodes: &[AtomNode],
        bfactor_mode: BFactorMode,
        params: &LensParams,
    ) -> f32 {
        match self {
            Self::BuriedSasa => (terr.buried_sasa_a2 / params.territory_buried_sasa_max_a2.max(1e-3)).clamp(0.0, 1.0),
            Self::Mobility | Self::Rigidity => {
                if terr.patch_atoms.is_empty() {
                    0.5
                } else {
                    let sum: f32 = terr
                        .patch_atoms
                        .iter()
                        .filter_map(|idx| nodes.get(idx.0 as usize))
                        .map(|n| self.atom_norm(n, bfactor_mode, params))
                        .sum();
                    (sum / terr.patch_atoms.len() as f32).clamp(0.0, 1.0)
                }
            }
            Self::AliphaticScore => self.territory_raw(terr, nodes).clamp(0.0, 1.0),
            Self::ClashSeverity => {
                (self.territory_raw(terr, nodes) / params.clash_severity_max_a.max(1e-3)).clamp(0.0, 1.0)
            }
        }
    }

    /// Formats a raw metric value with its appropriate precision and unit for text labels.
    pub fn format_value(self, raw: f32) -> String {
        match self {
            Self::BuriedSasa => format!("{raw:.1}Å²"),
            Self::Mobility | Self::Rigidity => format!("{raw:.1}"),
            Self::AliphaticScore => format!("{raw:.2}"),
            Self::ClashSeverity => format!("{raw:.2}Å"),
        }
    }

    /// Generates 4 sampled legend swatches `(color, label)` for this continuous metric.
    pub fn legend_entries(self, bfactor_mode: BFactorMode, p: &LensParams, palette: &Palette) -> Vec<(Color32, String)> {
        let (cmap, labels) = match self {
            Self::Mobility | Self::Rigidity => {
                let labels = match bfactor_mode {
                    BFactorMode::AlphaFold => [
                        format!("Very High (≥{:.0})", p.plddt_very_high_min),
                        format!("Confident (~{:.0})", p.plddt_confident_min),
                        format!("Low (~{:.0})", p.plddt_low_min),
                        format!("Very Low (<{:.0})", p.plddt_low_min),
                    ],
                    BFactorMode::Crystallographic => [
                        format!("Rigid (≤{:.0} Å²)", p.bfactor_rigid_max_a2),
                        format!("Ordered (~{:.0} Å²)", p.bfactor_ordered_max_a2),
                        format!("Flexible (~{:.0} Å²)", p.bfactor_flexible_max_a2),
                        format!("Labile (>{:.0} Å²)", p.bfactor_flexible_max_a2),
                    ],
                };
                (&palette.mobility, labels)
            }
            Self::BuriedSasa => {
                let max = p.atom_buried_sasa_max_a2;
                let labels = [
                    "0 Å² (Rim)".into(),
                    format!("~{:.0} Å²", max / 3.0),
                    format!("~{:.0} Å²", 2.0 * max / 3.0),
                    format!("≥{max:.0} Å² (Core)"),
                ];
                (&palette.mobility, labels)
            }
            Self::AliphaticScore => {
                let labels = ["0.0 (Polar)".into(), "0.33".into(), "0.67".into(), "1.0 (Aliphatic)".into()];
                (&palette.mobility, labels)
            }
            Self::ClashSeverity => {
                let max = p.clash_severity_max_a;
                let labels = [
                    "0.0 Å (Clean)".into(),
                    format!("{:.2} Å", max / 3.0),
                    format!("{:.2} Å", 2.0 * max / 3.0),
                    format!("≥{max:.2} Å (Severe)"),
                ];
                (&palette.charge, labels)
            }
        };
        cmap.swatches::<4>().into_iter().zip(labels).collect()
    }
}

/// Normalized `[0, 1]` scalar projection for a node sizing mode.
pub fn node_scalar_t(node: &AtomNode, mode: AtomSizeMode, bfactor_mode: BFactorMode, params: &LensParams) -> f32 {
    match mode {
        AtomSizeMode::Fixed => 0.5,
        AtomSizeMode::Off => 0.0,
        AtomSizeMode::SqrtBuriedSasa => ScalarMetric::BuriedSasa.atom_norm(node, bfactor_mode, params).sqrt(),
        _ => mode
            .scalar_metric()
            .map_or(0.5, |m| m.atom_norm(node, bfactor_mode, params)),
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
        AtomSizeMode::ClashSeverity => 1.0 + ScalarMetric::ClashSeverity.atom_raw(node) * 2.2,
        AtomSizeMode::SqrtBuriedSasa => 0.65 + 0.25 * ScalarMetric::BuriedSasa.atom_raw(node).max(1.5).sqrt(),
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
            let ra = nodes
                .get(edge.a.0 as usize)
                .map_or(0.5, |n| ScalarMetric::Rigidity.atom_norm(n, bfactor_mode, params));
            let rb = nodes
                .get(edge.b.0 as usize)
                .map_or(0.5, |n| ScalarMetric::Rigidity.atom_norm(n, bfactor_mode, params));
            0.8 + 2.4 * (0.5 * (ra + rb))
        }
        EdgeWidthLens::Proximity3D => {
            let span = (params.edge_dist_max_a - params.edge_dist_min_a).max(1e-3);
            let t = ((params.edge_dist_max_a - edge.dist_3d_a) / span).clamp(0.0, 1.0);
            0.8 + 2.4 * t
        }
        EdgeWidthLens::ClashSeverity => {
            let t = (edge.overlap_a / params.clash_severity_max_a.max(1e-3)).clamp(0.0, 1.0);
            0.9 + 2.6 * t
        }
        EdgeWidthLens::Strain2D => {
            let t = (edge.strain_2d_a.abs() / params.edge_strain_max_a.max(1e-3)).clamp(0.0, 1.0);
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
        TerritoryStrokeLens::BuriedSasa => {
            0.9 + 1.8 * ScalarMetric::BuriedSasa.territory_norm(terr, nodes, bfactor_mode, params)
        }
        TerritoryStrokeLens::UnsatisfiedPolar => {
            if terr.has_unsatisfied_polar { 2.4 } else { 1.1 }
        }
        TerritoryStrokeLens::AvgRigidity => {
            0.8 + 2.0 * ScalarMetric::Rigidity.territory_norm(terr, nodes, bfactor_mode, params)
        }
    };
    if selected { base + 0.8 } else { base }
}

/// Screen stroke width (pixels) of a 3D Cα cartoon ribbon in the Complex workspace.
pub fn complex_cartoon_width_px(zoom: f32, width_scale: f32, draw: &AtomDrawParams) -> f32 {
    (zoom * draw.complex_cartoon_width_per_zoom * width_scale).max(draw.min_complex_cartoon_width_px)
}

/// Screen radius (pixels) of a 3D interfacial heavy-atom sphere in the Complex workspace.
pub fn complex_sphere_radius_px(
    node: &AtomNode,
    vdw_radius_a: f32,
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
    let base = vdw_radius_a * zoom * draw.complex_sphere_radius_per_zoom;
    let scale = match mode {
        AtomSizeMode::Fixed => fixed_scale,
        AtomSizeMode::Off => 0.0,
        _ => 0.65 + 0.85 * node_scalar_t(node, mode, bfactor_mode, lenses),
    };
    (base * scale).max(draw.min_atom_radius_px)
}
