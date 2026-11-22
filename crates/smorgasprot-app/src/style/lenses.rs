//! Color lenses: per-atom fill colors and their matching legends.
//! Reads thresholds from [`LensParams`], discrete colors from domain sub-palettes, and continuous gradients from [`ColorMap`].

use crate::dataset::AtomMeta;
use crate::state::{BFactorMode, ColorLens, LensParams};
use crate::style::{ContactPalette, Palette};
use egui::Color32;
use smorgasprot_core::{AtomNode, EdgeKind, InterfaceSide};

/// Fill color for an interface atom under the active lens.
pub fn atom_fill_color(
    node: &AtomNode,
    meta: &AtomMeta,
    lens: ColorLens,
    bfactor_mode: BFactorMode,
    params: &LensParams,
    palette: &Palette,
) -> Color32 {
    match lens {
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
        ColorLens::Chemistry => {
            let ch = &palette.aa_chemistry;
            if node.aliphatic_score >= params.aliphatic_threshold {
                return ch.aliphatic;
            }
            match meta.res_name.as_str() {
                "PHE" | "TYR" | "TRP" => ch.aromatic,
                "ARG" | "LYS" | "HIS" => ch.positive,
                "ASP" | "GLU" => ch.negative,
                "GLY" | "PRO" => ch.gly_pro,
                _ => ch.polar,
            }
        }
        ColorLens::ModernCpk => palette.elements.color(meta.element),
        ColorLens::Bfactor => match bfactor_mode {
            // pLDDT: inverted range (high confidence = 0.0 -> low confidence = 1.0).
            BFactorMode::AlphaFold => {
                palette.mobility.sample_range(node.b_factor, params.plddt_very_high_min, params.plddt_low_min)
            }
            // B-factor: low rigid B-factor = 0.0 -> high labile B-factor = 1.0.
            BFactorMode::Crystallographic => {
                palette.mobility.sample_range(node.b_factor, params.bfactor_rigid_max_a2, params.bfactor_flexible_max_a2)
            }
        },
    }
}

/// Legend entries `(color, label)` for the active lens; group names label the side colors.
pub fn legend(
    lens: ColorLens,
    bfactor_mode: BFactorMode,
    group_a: &str,
    group_b: &str,
    params: &LensParams,
    palette: &Palette,
) -> Vec<(Color32, String)> {
    match lens {
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
        ColorLens::Bfactor => {
            let labels = mobility_swatch_labels(bfactor_mode, params);
            palette.mobility.swatches::<4>().into_iter().zip(labels).collect()
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

/// Color encoding for a contact edge (arcs and inspector tags).
pub fn contact_color(kind: EdgeKind, contacts: &ContactPalette) -> Color32 {
    contacts.color(kind)
}

/// Stroke width (px) for a contact arc; specific interactions draw heavier.
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
