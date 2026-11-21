//! Color lenses: per-atom fill colors and their matching legends.
//! Each lens defines its categories once, so atom colors and legend entries cannot drift apart.

use crate::dataset::AtomMeta;
use crate::state::{BFactorMode, ColorLens};
use crate::style::palette;
use egui::Color32;
use smorgasprot_core::{AtomNode, EdgeKind, Element, InterfaceSide};

/// Aliphatic score above which an atom is colored as aliphatic in the Chemistry lens.
const ALIPHATIC_THRESHOLD: f32 = 0.75;
/// Overlap (Å) above which an atom counts as clashing in the Interactions lens.
const CLASH_THRESHOLD_A: f32 = 0.05;

/// Fill color for an interface atom under the active lens.
pub fn atom_fill_color(node: &AtomNode, meta: &AtomMeta, lens: ColorLens, bfactor_mode: BFactorMode) -> Color32 {
    match lens {
        ColorLens::Interactions => {
            // Priority: clash > salt bridge > H-bond > plain side color.
            if worst_clash_overlap(node) > CLASH_THRESHOLD_A {
                palette::CLASH
            } else if has_cross_interaction(node, EdgeKind::SaltBridge) {
                palette::SALT_BRIDGE
            } else if has_cross_interaction(node, EdgeKind::HBond) {
                palette::HBOND
            } else if node.role == InterfaceSide::SideA {
                palette::SIDE_A
            } else {
                palette::SIDE_B_LIGHT
            }
        }
        ColorLens::Chemistry => {
            if node.aliphatic_score >= ALIPHATIC_THRESHOLD {
                return palette::ALIPHATIC;
            }
            match meta.res_name.as_str() {
                "PHE" | "TYR" | "TRP" => palette::AROMATIC,
                "ARG" | "LYS" | "HIS" => palette::POSITIVE,
                "ASP" | "GLU" => palette::NEGATIVE,
                "GLY" | "PRO" => palette::GLY_PRO,
                _ => palette::POLAR,
            }
        }
        ColorLens::ModernCpk => match meta.element {
            Element::C => palette::CPK_C,
            Element::N => palette::CPK_N,
            Element::O => palette::CPK_O,
            Element::S => palette::CPK_S,
            Element::P => palette::CPK_P,
            _ => palette::CPK_OTHER,
        },
        ColorLens::Bfactor => palette::MOBILITY_BINS[mobility_bin(node.b_factor, bfactor_mode)],
    }
}

/// Legend entries `(color, label)` for the active lens; group names label the side colors.
pub fn legend(lens: ColorLens, bfactor_mode: BFactorMode, group_a: &str, group_b: &str) -> Vec<(Color32, String)> {
    let entries: Vec<(Color32, &str)> = match lens {
        ColorLens::Interactions => vec![
            (palette::CLASH, "VdW Clash"),
            (palette::HBOND, "H-Bond"),
            (palette::SALT_BRIDGE, "Salt Bridge"),
            (palette::SIDE_A, group_a),
            (palette::SIDE_B_LIGHT, group_b),
        ],
        ColorLens::Chemistry => vec![
            (palette::ALIPHATIC, "Aliphatic"),
            (palette::AROMATIC, "Aromatic"),
            (palette::POLAR, "Polar"),
            (palette::POSITIVE, "Positive"),
            (palette::NEGATIVE, "Negative"),
            (palette::GLY_PRO, "Gly / Pro"),
        ],
        ColorLens::ModernCpk => vec![
            (palette::CPK_C, "Carbon (C)"),
            (palette::CPK_N, "Nitrogen (N)"),
            (palette::CPK_O, "Oxygen (O)"),
            (palette::CPK_S, "Sulfur (S)"),
        ],
        ColorLens::Bfactor => {
            let labels = mobility_bin_labels(bfactor_mode);
            palette::MOBILITY_BINS.iter().copied().zip(labels).collect()
        }
    };
    entries.into_iter().map(|(c, l)| (c, l.to_string())).collect()
}

/// Bins a B-factor (Å²) or pLDDT value into `0` (ordered/confident) … `3` (mobile/low confidence).
fn mobility_bin(value: f32, mode: BFactorMode) -> usize {
    match mode {
        // pLDDT: higher is more confident.
        BFactorMode::AlphaFold => match value {
            v if v >= 90.0 => 0,
            v if v >= 70.0 => 1,
            v if v >= 50.0 => 2,
            _ => 3,
        },
        // B-factor: lower is more rigid.
        BFactorMode::Crystallographic => match value {
            v if v <= 30.0 => 0,
            v if v <= 50.0 => 1,
            v if v <= 70.0 => 2,
            _ => 3,
        },
    }
}

/// Legend labels matching [`mobility_bin`] thresholds.
fn mobility_bin_labels(mode: BFactorMode) -> [&'static str; 4] {
    match mode {
        BFactorMode::AlphaFold => ["Very High (>90)", "Confident (70-90)", "Low (50-70)", "Very Low (<50)"],
        BFactorMode::Crystallographic => ["Rigid (≤30)", "Ordered (30-50)", "Flexible (50-70)", "Labile (>70)"],
    }
}

/// Color encoding for a contact edge (arcs and inspector tags).
pub fn contact_color(kind: EdgeKind) -> Color32 {
    match kind {
        EdgeKind::Clash => palette::CLASH,
        EdgeKind::SaltBridge => palette::SALT_BRIDGE,
        EdgeKind::HBond => palette::HBOND,
        EdgeKind::VanDerWaalsContact | EdgeKind::SameResidue => palette::CONTACT,
    }
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
