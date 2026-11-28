//! Color visual-channel encodings: node fill, node border stroke, edge stroke, territory fill, labels, and legends.
//! Projects categorical rules and unified [`ScalarMetric`]s through [`Palette`] and [`ColorMap`].

use crate::dataset::AtomMeta;
use crate::state::{
    BFactorMode, ChainColorLens, ColorLens, EdgeColorLens, LabelBuilder, LabelValueChannel, LensParams,
    NodeStrokeColorLens, ResidueNameStyle, ScalarMetric, TerritoryColorLens,
};
use crate::style::palette::darken;
use crate::style::{ContactPalette, Palette};
use egui::Color32;
use smorgasprot_core::{
    AminoAcid, AtomNode, BondKind, EdgeKind, InterfaceEdge, InterfaceSide, ResidueTerritory, print_three_letter_code,
    residue_full_name,
};

/// Base fill color for a 3D chain molecular surface under the active [`ChainColorLens`].
pub fn chain_surface_color(
    chain_idx: usize,
    side: InterfaceSide,
    lens: ChainColorLens,
    fixed_color: Color32,
    palette: &Palette,
) -> Color32 {
    match lens {
        ChainColorLens::Fixed => fixed_color,
        ChainColorLens::Off => Color32::TRANSPARENT,
        ChainColorLens::InterfaceSide => palette.sides.side(side),
        ChainColorLens::ChainIndex => palette.categorical(chain_idx),
        _ => palette.sides.side(side),
    }
}

/// Stroke color for a 3D chain Cα cartoon ribbon under the active [`ChainColorLens`].
pub fn chain_cartoon_color(
    chain_idx: usize,
    side: InterfaceSide,
    lens: ChainColorLens,
    fixed_color: Color32,
    palette: &Palette,
) -> Color32 {
    match lens {
        ChainColorLens::Fixed => fixed_color,
        ChainColorLens::Off => Color32::TRANSPARENT,
        ChainColorLens::InterfaceSide => palette.sides.side_dark(side),
        ChainColorLens::ChainIndex => darken(palette.categorical(chain_idx), 0.20),
        _ => palette.sides.side_dark(side),
    }
}

/// Samples the appropriate continuous [`ColorMap`] on `palette` for a normalized `[0, 1]` [`ScalarMetric`].
pub(crate) fn sample_metric_color(metric: ScalarMetric, t: f32, palette: &Palette) -> Color32 {
    match metric {
        ScalarMetric::ClashSeverity => palette.charge.sample(t),
        ScalarMetric::BuriedSasa
        | ScalarMetric::Mobility
        | ScalarMetric::Rigidity
        | ScalarMetric::AliphaticScore => palette.mobility.sample(t),
    }
}

/// Fill color for an interface atom node under the active [`ColorLens`].
#[allow(clippy::too_many_arguments)]
pub fn atom_fill_color(
    node: &AtomNode,
    meta: &AtomMeta,
    lens: ColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    side_a_on_top: bool,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    if let Some(metric) = lens.scalar_metric() {
        return sample_metric_color(metric, metric.atom_norm(node, bfactor_mode, params), palette);
    }
    match lens {
        ColorLens::Fixed => fixed_color,
        ColorLens::Off => Color32::TRANSPARENT,
        ColorLens::Interactions => {
            let c = &palette.contacts;
            // Priority: clash > salt bridge > H-bond > plain side color (top group primary, bottom lighter).
            let top_side = if side_a_on_top { InterfaceSide::SideA } else { InterfaceSide::SideB };
            if ScalarMetric::ClashSeverity.atom_raw(node) > params.clash_threshold_a {
                c.clash
            } else if has_cross_interaction(node, EdgeKind::SaltBridge) {
                c.salt_bridge
            } else if has_cross_interaction(node, EdgeKind::HBond) {
                c.hbond
            } else if node.side == top_side {
                palette.sides.side(node.side)
            } else {
                palette.sides.side_light(node.side)
            }
        }
        ColorLens::AaChemistry => aa_chemistry_color(&meta.res_name, node.aliphatic_score, params, palette),
        ColorLens::ModernCpk => palette.elements.color(meta.element),
        ColorLens::InterfaceSide | ColorLens::InterfaceMembership | ColorLens::ChainIndex => {
            palette.sides.side(node.side)
        }
        ColorLens::Bfactor | ColorLens::BuriedSasa | ColorLens::AliphaticScore | ColorLens::ClashSeverity => {
            unreachable!()
        }
    }
}

/// Evaluates a [`ColorLens`] for any atom in the complex, using its [`AtomNode`] when interfacial.
#[allow(clippy::too_many_arguments)]
pub fn complex_atom_color(
    atom: &smorgasprot_core::AtomRecord,
    res_name: &str,
    chain_idx: usize,
    side: InterfaceSide,
    node: Option<&AtomNode>,
    lens: ColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    if let Some(metric) = lens.scalar_metric() {
        let t = crate::style::sizing::complex_atom_metric_norm(metric, atom, node, bfactor_mode, params);
        return sample_metric_color(metric, t, palette);
    }
    match lens {
        ColorLens::Fixed => fixed_color,
        ColorLens::Off => Color32::TRANSPARENT,
        ColorLens::InterfaceSide => palette.sides.side(side),
        ColorLens::ChainIndex => palette.categorical(chain_idx),
        ColorLens::InterfaceMembership => match node {
            Some(n) => palette.sides.side(n.side),
            None => palette.sides.unassigned,
        },
        ColorLens::Interactions => match node {
            Some(n) => {
                let c = &palette.contacts;
                if ScalarMetric::ClashSeverity.atom_raw(n) > params.clash_threshold_a {
                    c.clash
                } else if has_cross_interaction(n, EdgeKind::SaltBridge) {
                    c.salt_bridge
                } else if has_cross_interaction(n, EdgeKind::HBond) {
                    c.hbond
                } else if n.side == InterfaceSide::SideA {
                    palette.sides.side_a
                } else {
                    palette.sides.side_light(InterfaceSide::SideB)
                }
            }
            None => palette.sides.side(side),
        },
        ColorLens::AaChemistry => match node {
            Some(n) => aa_chemistry_color(res_name, n.aliphatic_score, params, palette),
            None => {
                let score = if matches!(res_name, "ALA" | "VAL" | "ILE" | "LEU" | "MET") {
                    params.aliphatic_threshold
                } else {
                    0.0
                };
                aa_chemistry_color(res_name, score, params, palette)
            }
        },
        ColorLens::ModernCpk => palette.elements.color(atom.element),
        ColorLens::Bfactor | ColorLens::BuriedSasa | ColorLens::AliphaticScore | ColorLens::ClashSeverity => {
            unreachable!()
        }
    }
}

/// Border stroke color for an interface atom node under the active [`NodeStrokeColorLens`].
#[allow(clippy::too_many_arguments)]
pub fn atom_stroke_color(
    node: &AtomNode,
    meta: &AtomMeta,
    lens: NodeStrokeColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    side_a_on_top: bool,
    hovered: bool,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    if hovered {
        return Color32::WHITE;
    }
    match lens {
        NodeStrokeColorLens::Fixed => fixed_color,
        NodeStrokeColorLens::Off => Color32::TRANSPARENT,
        NodeStrokeColorLens::DefaultInk => palette.chrome.ink,
        NodeStrokeColorLens::UnsatisfiedPolar => {
            if node.is_unsatisfied_buried_polar {
                palette.chrome.warning
            } else {
                palette.chrome.ink
            }
        }
        NodeStrokeColorLens::Interactions => {
            atom_fill_color(node, meta, ColorLens::Interactions, fixed_color, bfactor_mode, side_a_on_top, params, palette)
        }
        NodeStrokeColorLens::Chemistry => {
            atom_fill_color(node, meta, ColorLens::AaChemistry, fixed_color, bfactor_mode, side_a_on_top, params, palette)
        }
        NodeStrokeColorLens::ModernCpk => {
            atom_fill_color(node, meta, ColorLens::ModernCpk, fixed_color, bfactor_mode, side_a_on_top, params, palette)
        }
        NodeStrokeColorLens::InterfaceSide => palette.sides.side_dark(node.side),
        NodeStrokeColorLens::Bfactor => {
            atom_fill_color(node, meta, ColorLens::Bfactor, fixed_color, bfactor_mode, side_a_on_top, params, palette)
        }
    }
}

/// Stroke color for a contact edge under the active [`EdgeColorLens`].
pub fn edge_stroke_color(
    edge: &InterfaceEdge,
    nodes: &[AtomNode],
    lens: EdgeColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    match lens {
        EdgeColorLens::Fixed => fixed_color,
        EdgeColorLens::Off => Color32::TRANSPARENT,
        EdgeColorLens::InteractionKind => palette.contacts.color(edge.kind),
        EdgeColorLens::Strain2D => {
            palette.charge.sample_range(edge.strain_2d_a, -params.edge_strain_max_a, params.edge_strain_max_a)
        }
        EdgeColorLens::EndpointMobility => {
            let ma = nodes
                .get(edge.a.0 as usize)
                .map_or(0.5, |n| ScalarMetric::Mobility.atom_norm(n, bfactor_mode, params));
            let mb = nodes
                .get(edge.b.0 as usize)
                .map_or(0.5, |n| ScalarMetric::Mobility.atom_norm(n, bfactor_mode, params));
            palette.mobility.sample(0.5 * (ma + mb))
        }
        EdgeColorLens::Distance3D => {
            palette.mobility.sample_range(edge.dist_3d_a, params.edge_dist_min_a, params.edge_dist_max_a)
        }
    }
}

/// Fill color for a [`ResidueTerritory`] polygon under the active [`TerritoryColorLens`].
pub fn territory_fill_color(
    terr: &ResidueTerritory,
    nodes: &[AtomNode],
    metas: &[AtomMeta],
    lens: TerritoryColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    if let Some(metric) = lens.scalar_metric() {
        return sample_metric_color(metric, metric.territory_norm(terr, nodes, bfactor_mode, params), palette);
    }
    match lens {
        TerritoryColorLens::Fixed => fixed_color,
        TerritoryColorLens::Off => Color32::TRANSPARENT,
        TerritoryColorLens::InterfaceSide => palette.sides.side(terr.side),
        TerritoryColorLens::Chemistry => {
            let res_name = terr
                .member_atoms
                .first()
                .and_then(|idx| metas.get(idx.0 as usize))
                .map_or("", |m| m.res_name.as_str());
            let avg_aliphatic = ScalarMetric::AliphaticScore.territory_raw(terr, nodes);
            aa_chemistry_color(res_name, avg_aliphatic, params, palette)
        }
        TerritoryColorLens::UnsatisfiedPolar => {
            if terr.unsatisfied_polar_buried_sasa_a2 > 0.0 {
                palette.chrome.warning
            } else {
                palette.sides.side(terr.side)
            }
        }
        TerritoryColorLens::BuriedSasa | TerritoryColorLens::AvgMobility => unreachable!(),
    }
}

/// Formats a residue name string (e.g. `"GLU"`, `"KEF"`) according to [`ResidueNameStyle`].
fn format_res_name(res_name: &str, style: ResidueNameStyle) -> Option<String> {
    match style {
        ResidueNameStyle::Off => None,
        ResidueNameStyle::OneLetter => {
            if let Some(aa) = AminoAcid::from_three_letter(res_name) {
                Some(aa.one_letter().to_string())
            } else {
                Some(res_name.trim().to_string())
            }
        }
        ResidueNameStyle::ThreeLetter => {
            let mut bytes = [b' '; 3];
            for (i, b) in res_name.trim().bytes().take(3).enumerate() {
                bytes[i] = b;
            }
            Some(print_three_letter_code(&bytes))
        }
        ResidueNameStyle::FullName => Some(residue_full_name(res_name)),
    }
}

/// Builds the display text for a [`ResidueTerritory`] label from [`LabelBuilder`].
pub fn format_territory_label(
    terr: &ResidueTerritory,
    nodes: &[AtomNode],
    metas: &[AtomMeta],
    builder: &LabelBuilder,
) -> Option<String> {
    if !builder.enabled {
        return None;
    }
    let first_meta = terr.member_atoms.first().and_then(|idx| metas.get(idx.0 as usize));
    let res_name = first_meta.map_or("", |m| m.res_name.as_str());
    let res_num = first_meta.map_or(0, |m| m.res_num);

    let mut head = String::new();
    if builder.show_chain {
        head.push_str(terr.chain_id.as_str());
    }
    if let Some(name_str) = format_res_name(res_name, builder.res_name_style) {
        if !head.is_empty() {
            head.push(':');
        }
        head.push_str(&name_str);
    }
    if builder.show_res_num {
        let sep = match builder.res_name_style {
            ResidueNameStyle::OneLetter => "",
            ResidueNameStyle::Off => {
                if head.is_empty() { "" } else { ":" }
            }
            ResidueNameStyle::ThreeLetter | ResidueNameStyle::FullName => {
                if head.is_empty() { "" } else { " " }
            }
        };
        head.push_str(sep);
        head.push_str(&res_num.to_string());
    }

    let mut header_parts = Vec::new();
    if !head.is_empty() {
        header_parts.push(head);
    }
    if builder.show_sec_struct && !terr.label_bottom.is_empty() {
        header_parts.push(format!("({})", terr.label_bottom));
    }
    let header = header_parts.join(" ");
    match (header.is_empty(), format_territory_value(terr, nodes, builder.value_channel)) {
        (true, None) => None,
        (false, None) => Some(header),
        (true, Some(val_str)) => Some(val_str),
        (false, Some(val_str)) => Some(format!("{header}\n{val_str}")),
    }
}

/// Builds the display text for an [`AtomNode`] label from [`LabelBuilder`].
pub fn format_atom_label(node: &AtomNode, meta: &AtomMeta, builder: &LabelBuilder) -> Option<String> {
    if !builder.enabled {
        return None;
    }
    let mut head = String::new();
    if builder.show_chain {
        head.push_str(meta.chain_id.as_str());
    }
    if let Some(name_str) = format_res_name(&meta.res_name, builder.res_name_style) {
        if !head.is_empty() {
            head.push(':');
        }
        head.push_str(&name_str);
    }
    if builder.show_res_num {
        let sep = match builder.res_name_style {
            ResidueNameStyle::OneLetter => "",
            ResidueNameStyle::Off => {
                if head.is_empty() { "" } else { ":" }
            }
            ResidueNameStyle::ThreeLetter | ResidueNameStyle::FullName => {
                if head.is_empty() { "" } else { " " }
            }
        };
        head.push_str(sep);
        head.push_str(&meta.res_num.to_string());
    }
    if builder.show_atom_name {
        if !head.is_empty() {
            head.push(':');
        }
        head.push_str(&meta.atom_name);
    }

    match (head.is_empty(), format_atom_value(node, builder.value_channel)) {
        (true, None) => None,
        (false, None) => Some(head),
        (true, Some(val_str)) => Some(val_str),
        (false, Some(val_str)) => Some(format!("{head}\n{val_str}")),
    }
}

fn format_territory_value(terr: &ResidueTerritory, nodes: &[AtomNode], ch: LabelValueChannel) -> Option<String> {
    if let Some(metric) = ch.scalar_metric() {
        return Some(metric.format_value(metric.territory_raw(terr, nodes)));
    }
    match ch {
        LabelValueChannel::Off => None,
        LabelValueChannel::ContactCount => {
            let count: usize = terr
                .member_atoms
                .iter()
                .filter_map(|idx| nodes.get(idx.0 as usize))
                .map(|n| n.interactions.iter().filter(|i| i.is_cross_interface).count())
                .sum();
            Some(format!("({count})"))
        }
        _ => unreachable!(),
    }
}

fn format_atom_value(node: &AtomNode, ch: LabelValueChannel) -> Option<String> {
    if let Some(metric) = ch.scalar_metric() {
        return Some(metric.format_value(metric.atom_raw(node)));
    }
    match ch {
        LabelValueChannel::Off => None,
        LabelValueChannel::ContactCount => {
            let count = node.interactions.iter().filter(|i| i.is_cross_interface).count();
            Some(format!("({count})"))
        }
        _ => unreachable!(),
    }
}

pub(crate) fn aa_chemistry_color(res_name: &str, aliphatic_score: f32, params: &LensParams, palette: &Palette) -> Color32 {
    let ch = &palette.aa_chemistry;
    if aliphatic_score >= params.aliphatic_threshold {
        return ch.aliphatic;
    }
    match res_name {
        "PHE" | "TYR" | "TRP" => ch.aromatic,
        "ARG" | "LYS" | "HIS" => ch.positive,
        "ASP" | "GLU" => ch.negative,
        "GLY" | "PRO" => ch.gly_pro,
        _ => ch.polar,
    }
}

/// Legend entries `(color, label)` for the active node fill lens; group names label the side colors.
pub fn legend(
    lens: ColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    group_a: &str,
    group_b: &str,
    params: &LensParams,
    palette: &Palette,
) -> Vec<(Color32, String)> {
    if let Some(metric) = lens.scalar_metric() {
        return metric.legend_entries(bfactor_mode, params, palette);
    }
    match lens {
        ColorLens::Fixed => vec![(fixed_color, "Fixed Node Color".into())],
        ColorLens::Off => Vec::new(),
        ColorLens::Interactions => palette.contacts.legend_entries(&palette.sides, group_a, group_b),
        ColorLens::AaChemistry => palette.aa_chemistry.legend_entries(),
        ColorLens::ModernCpk => palette.elements.legend_entries(),
        ColorLens::InterfaceSide | ColorLens::ChainIndex => palette.sides.legend_entries(group_a, group_b),
        ColorLens::InterfaceMembership => vec![
            (palette.sides.side_a, format!("{group_a} (Interface)")),
            (palette.sides.side_b, format!("{group_b} (Interface)")),
            (palette.sides.unassigned, "Non-interfacial".into()),
        ],
        ColorLens::Bfactor | ColorLens::BuriedSasa | ColorLens::AliphaticScore | ColorLens::ClashSeverity => {
            unreachable!()
        }
    }
}

/// Color encoding for a contact edge kind (inspector tags and default edge lens).
pub fn contact_color(kind: EdgeKind, contacts: &ContactPalette) -> Color32 {
    contacts.color(kind)
}

/// Default stroke width (px) for a contact arc by interaction kind.
pub fn contact_width(kind: EdgeKind) -> f32 {
    match kind {
        EdgeKind::Clash => 2.2,
        EdgeKind::SaltBridge => 2.0,
        EdgeKind::HBond => 1.8,
        EdgeKind::Covalent(BondKind::Disulfide) => 1.6,
        EdgeKind::Covalent(BondKind::Backbone | BondKind::Sidechain | BondKind::Other) => 1.0,
        EdgeKind::VanDerWaalsContact => 1.0,
    }
}

/// Short human-readable name for a contact kind.
pub fn contact_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Clash => "Clash",
        EdgeKind::SaltBridge => "Salt bridge",
        EdgeKind::HBond => "H-bond",
        EdgeKind::Covalent(BondKind::Backbone) => "Backbone bond",
        EdgeKind::Covalent(BondKind::Sidechain) => "Sidechain bond",
        EdgeKind::Covalent(BondKind::Disulfide) => "Disulfide",
        EdgeKind::Covalent(BondKind::Other) => "Other covalent bond",
        EdgeKind::VanDerWaalsContact => "vdW contact",
    }
}

/// Largest steric overlap (Å) among an atom's clash interactions, or `0`.
pub fn worst_clash_overlap(node: &AtomNode) -> f32 {
    if node.interactions.iter().any(|i| i.kind == EdgeKind::Clash) {
        1.0
    } else {
        0.0
    }
}

/// Whether an atom has at least one cross-interface interaction of `kind`.
fn has_cross_interaction(node: &AtomNode, kind: EdgeKind) -> bool {
    node.interactions.iter().any(|i| i.is_cross_interface && i.kind == kind)
}
