//! Color visual-channel encodings: node fill, node border stroke, edge stroke, territory fill, labels, and legends.
//! Projects categorical and continuous scalar metrics through [`Palette`] and [`ColorMap`].

use crate::dataset::AtomMeta;
use crate::state::{
    BFactorMode, ColorLens, EdgeColorLens, LabelBuilder, LabelValueChannel, LensParams, NodeStrokeColorLens,
    ResidueNameStyle, TerritoryColorLens,
};
use crate::style::sizing::node_mobility_t;
use crate::style::{ContactPalette, Palette};
use egui::Color32;
use smorgasprot_core::{
    AminoAcid, AtomNode, EdgeKind, InterfaceEdge, InterfaceSide, ResidueTerritory, print_three_letter_code,
    residue_full_name,
};

/// Fill color for an interface atom node under the active [`ColorLens`].
pub fn atom_fill_color(
    node: &AtomNode,
    meta: &AtomMeta,
    lens: ColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    match lens {
        ColorLens::Fixed => fixed_color,
        ColorLens::Off => Color32::TRANSPARENT,
        ColorLens::Interactions => {
            let c = &palette.contacts;
            // Priority: clash > salt bridge > H-bond > plain side color.
            if worst_clash_overlap(node) > params.clash_threshold_a {
                c.clash
            } else if has_cross_interaction(node, EdgeKind::SaltBridge) {
                c.salt_bridge
            } else if has_cross_interaction(node, EdgeKind::HBond) {
                c.hbond
            } else if node.role == InterfaceSide::SideA {
                palette.sides.side_a
            } else {
                palette.sides.side_light(InterfaceSide::SideB)
            }
        }
        ColorLens::Chemistry => aa_chemistry_color(&meta.res_name, node.aliphatic_score, params, palette),
        ColorLens::ModernCpk => palette.elements.color(meta.element),
        ColorLens::InterfaceSide => palette.sides.side(node.role),
        ColorLens::Bfactor => palette.mobility.sample(node_mobility_t(node, bfactor_mode, params)),
        ColorLens::BuriedSasa => palette.mobility.sample_range(node.buried_sasa_a2, 0.0, 25.0),
        ColorLens::AliphaticScore => palette.mobility.sample(node.aliphatic_score),
        ColorLens::ClashSeverity => palette.charge.sample_range(worst_clash_overlap(node), 0.0, 0.6),
    }
}

/// Border stroke color for an interface atom node under the active [`NodeStrokeColorLens`].
pub fn atom_stroke_color(
    node: &AtomNode,
    meta: &AtomMeta,
    lens: NodeStrokeColorLens,
    fixed_color: Color32,
    bfactor_mode: BFactorMode,
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
            atom_fill_color(node, meta, ColorLens::Interactions, fixed_color, bfactor_mode, params, palette)
        }
        NodeStrokeColorLens::Chemistry => {
            atom_fill_color(node, meta, ColorLens::Chemistry, fixed_color, bfactor_mode, params, palette)
        }
        NodeStrokeColorLens::ModernCpk => {
            atom_fill_color(node, meta, ColorLens::ModernCpk, fixed_color, bfactor_mode, params, palette)
        }
        NodeStrokeColorLens::InterfaceSide => palette.sides.side_dark(node.role),
        NodeStrokeColorLens::Bfactor => {
            atom_fill_color(node, meta, ColorLens::Bfactor, fixed_color, bfactor_mode, params, palette)
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
        EdgeColorLens::Strain2D => palette.charge.sample_range(edge.strain_2d_a, -1.2, 1.2),
        EdgeColorLens::EndpointMobility => {
            let ma = nodes.get(edge.a.0 as usize).map_or(0.5, |n| node_mobility_t(n, bfactor_mode, params));
            let mb = nodes.get(edge.b.0 as usize).map_or(0.5, |n| node_mobility_t(n, bfactor_mode, params));
            palette.mobility.sample(0.5 * (ma + mb))
        }
        EdgeColorLens::Distance3D => palette.mobility.sample_range(edge.dist_3d_a, 2.3, 4.5),
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
    match lens {
        TerritoryColorLens::Fixed => fixed_color,
        TerritoryColorLens::Off => Color32::TRANSPARENT,
        TerritoryColorLens::InterfaceSide => palette.sides.side(terr.role),
        TerritoryColorLens::Chemistry => {
            let res_name = terr
                .patch_atoms
                .first()
                .and_then(|idx| metas.get(idx.0 as usize))
                .map_or("", |m| m.res_name.as_str());
            let avg_aliphatic = if terr.patch_atoms.is_empty() {
                0.0
            } else {
                let sum: f32 = terr
                    .patch_atoms
                    .iter()
                    .filter_map(|idx| nodes.get(idx.0 as usize))
                    .map(|n| n.aliphatic_score)
                    .sum();
                sum / terr.patch_atoms.len() as f32
            };
            aa_chemistry_color(res_name, avg_aliphatic, params, palette)
        }
        TerritoryColorLens::BuriedSasa => palette.mobility.sample_range(terr.buried_sasa_a2, 0.0, 90.0),
        TerritoryColorLens::AvgMobility => {
            if terr.patch_atoms.is_empty() {
                palette.sides.side(terr.role)
            } else {
                let sum: f32 = terr
                    .patch_atoms
                    .iter()
                    .filter_map(|idx| nodes.get(idx.0 as usize))
                    .map(|n| node_mobility_t(n, bfactor_mode, params))
                    .sum();
                palette.mobility.sample(sum / terr.patch_atoms.len() as f32)
            }
        }
        TerritoryColorLens::UnsatisfiedPolar => {
            if terr.has_unsatisfied_polar {
                palette.chrome.warning
            } else {
                palette.sides.side(terr.role)
            }
        }
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
    let first_meta = terr.patch_atoms.first().and_then(|idx| metas.get(idx.0 as usize));
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

    let mut parts = Vec::new();
    if !head.is_empty() {
        parts.push(head);
    }
    if builder.show_sec_struct && !terr.label_bottom.is_empty() {
        parts.push(format!("({})", terr.label_bottom));
    }
    if let Some(val_str) = format_territory_value(terr, nodes, builder.value_channel) {
        parts.push(val_str);
    }

    if parts.is_empty() { None } else { Some(parts.join(" ")) }
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

    let mut parts = Vec::new();
    if !head.is_empty() {
        parts.push(head);
    }
    if let Some(val_str) = format_atom_value(node, builder.value_channel) {
        parts.push(val_str);
    }

    if parts.is_empty() { None } else { Some(parts.join(" ")) }
}

fn format_territory_value(terr: &ResidueTerritory, nodes: &[AtomNode], ch: LabelValueChannel) -> Option<String> {
    match ch {
        LabelValueChannel::Off => None,
        LabelValueChannel::BuriedSasa => Some(format!("{:.1}Å²", terr.buried_sasa_a2)),
        LabelValueChannel::Bfactor => {
            if terr.patch_atoms.is_empty() {
                None
            } else {
                let sum: f32 = terr
                    .patch_atoms
                    .iter()
                    .filter_map(|idx| nodes.get(idx.0 as usize))
                    .map(|n| n.b_factor)
                    .sum();
                Some(format!("{:.1}", sum / terr.patch_atoms.len() as f32))
            }
        }
        LabelValueChannel::AliphaticScore => {
            if terr.patch_atoms.is_empty() {
                None
            } else {
                let sum: f32 = terr
                    .patch_atoms
                    .iter()
                    .filter_map(|idx| nodes.get(idx.0 as usize))
                    .map(|n| n.aliphatic_score)
                    .sum();
                Some(format!("{:.2}", sum / terr.patch_atoms.len() as f32))
            }
        }
        LabelValueChannel::ClashSeverity => {
            let worst = terr
                .patch_atoms
                .iter()
                .filter_map(|idx| nodes.get(idx.0 as usize))
                .map(worst_clash_overlap)
                .fold(0.0, f32::max);
            Some(format!("{worst:.2}Å"))
        }
        LabelValueChannel::ContactCount => {
            let count: usize = terr
                .patch_atoms
                .iter()
                .filter_map(|idx| nodes.get(idx.0 as usize))
                .map(|n| n.interactions.iter().filter(|i| i.is_cross_interface).count())
                .sum();
            Some(format!("({count})"))
        }
    }
}

fn format_atom_value(node: &AtomNode, ch: LabelValueChannel) -> Option<String> {
    match ch {
        LabelValueChannel::Off => None,
        LabelValueChannel::BuriedSasa => Some(format!("{:.1}Å²", node.buried_sasa_a2)),
        LabelValueChannel::Bfactor => Some(format!("{:.1}", node.b_factor)),
        LabelValueChannel::AliphaticScore => Some(format!("{:.2}", node.aliphatic_score)),
        LabelValueChannel::ClashSeverity => Some(format!("{:.2}Å", worst_clash_overlap(node))),
        LabelValueChannel::ContactCount => {
            let count = node.interactions.iter().filter(|i| i.is_cross_interface).count();
            Some(format!("({count})"))
        }
    }
}

fn aa_chemistry_color(res_name: &str, aliphatic_score: f32, params: &LensParams, palette: &Palette) -> Color32 {
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
    match lens {
        ColorLens::Fixed => vec![(fixed_color, "Fixed Node Color".into())],
        ColorLens::Off => Vec::new(),
        ColorLens::Interactions => {
            let c = &palette.contacts;
            vec![
                (c.clash, "VdW Clash".into()),
                (c.hbond, "H-Bond".into()),
                (c.salt_bridge, "Salt Bridge".into()),
                (palette.sides.side_a, group_a.into()),
                (palette.sides.side_light(InterfaceSide::SideB), group_b.into()),
            ]
        }
        ColorLens::Chemistry => {
            let ch = &palette.aa_chemistry;
            vec![
                (ch.aliphatic, "Aliphatic".into()),
                (ch.aromatic, "Aromatic".into()),
                (ch.polar, "Polar".into()),
                (ch.positive, "Positive".into()),
                (ch.negative, "Negative".into()),
                (ch.gly_pro, "Gly / Pro".into()),
            ]
        }
        ColorLens::ModernCpk => {
            let el = &palette.elements;
            vec![
                (el.c, "Carbon (C)".into()),
                (el.n, "Nitrogen (N)".into()),
                (el.o, "Oxygen (O)".into()),
                (el.s, "Sulfur (S)".into()),
            ]
        }
        ColorLens::InterfaceSide => vec![
            (palette.sides.side_a, group_a.into()),
            (palette.sides.side_b, group_b.into()),
        ],
        ColorLens::Bfactor => {
            let labels = mobility_swatch_labels(bfactor_mode, params);
            palette.mobility.swatches::<4>().into_iter().zip(labels).collect()
        }
        ColorLens::BuriedSasa => {
            let labels = ["0 Å² (Rim)".into(), "~8 Å²".into(), "~16 Å²".into(), "≥25 Å² (Core)".into()];
            palette.mobility.swatches::<4>().into_iter().zip(labels).collect()
        }
        ColorLens::AliphaticScore => {
            let labels = ["0.0 (Polar)".into(), "0.33".into(), "0.67".into(), "1.0 (Aliphatic)".into()];
            palette.mobility.swatches::<4>().into_iter().zip(labels).collect()
        }
        ColorLens::ClashSeverity => {
            let labels = ["0.0 Å (Clean)".into(), "0.2 Å".into(), "0.4 Å".into(), "≥0.6 Å (Severe)".into()];
            palette.charge.swatches::<4>().into_iter().zip(labels).collect()
        }
    }
}

/// Legend labels for 4 sampled swatches along the continuous mobility [`ColorMap`].
fn mobility_swatch_labels(mode: BFactorMode, p: &LensParams) -> [String; 4] {
    match mode {
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
        EdgeKind::VanDerWaalsContact | EdgeKind::SameResidue => 1.0,
    }
}

/// Short human-readable name for a contact kind.
pub fn contact_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Clash => "Clash",
        EdgeKind::SaltBridge => "Salt bridge",
        EdgeKind::HBond => "H-bond",
        EdgeKind::VanDerWaalsContact => "vdW contact",
        EdgeKind::SameResidue => "Same residue",
    }
}

/// Largest steric overlap (Å) among an atom's clash interactions, or `0`.
pub fn worst_clash_overlap(node: &AtomNode) -> f32 {
    node.interactions
        .iter()
        .filter(|i| i.kind == EdgeKind::Clash)
        .map(|i| i.overlap_a)
        .fold(0.0, f32::max)
}

/// Whether an atom has at least one cross-interface interaction of `kind`.
fn has_cross_interaction(node: &AtomNode, kind: EdgeKind) -> bool {
    node.interactions.iter().any(|i| i.is_cross_interface && i.kind == kind)
}
