//! Visual lenses for biochemical properties, contacts, CPK elements, and B-Factor / pLDDT mobility.

use crate::dataset::AtomMeta;
use egui::Color32;
use serde::{Deserialize, Serialize};
use smorgasprot_core::{AtomNode, EdgeKind, InterfaceSide};

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ColorLens {
    Interactions,
    Chemistry,
    ModernCpk,
    Bfactor,
}

impl ColorLens {
    pub const ALL: [Self; 4] = [
        Self::Interactions,
        Self::Chemistry,
        Self::ModernCpk,
        Self::Bfactor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Interactions => "1. Specific Interactions (H-Bonds / Salt Bridges / Clashes)",
            Self::Chemistry => "2. Chemical Category (Aliphatic / Polar / Charge / Aromatic)",
            Self::ModernCpk => "3. Modern CPK (Elemental Core Colors)",
            Self::Bfactor => "4. B-Factor & Crystallographic Mobility / pLDDT",
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum BFactorMode {
    Crystallographic,
    AlphaFold,
}

impl BFactorMode {
    pub const ALL: [Self; 2] = [Self::Crystallographic, Self::AlphaFold];

    pub fn label(self) -> &'static str {
        match self {
            Self::Crystallographic => "Crystallographic B-Factor (Rigid Blue <= 30 -> Labile Red > 70)",
            Self::AlphaFold => "AlphaFold pLDDT (High Blue > 90 -> Low Red < 50)",
        }
    }
}

/// Returns the worst steric clash overlap (in Å) on a given atom node.
pub fn worst_clash_on_node(node: &AtomNode) -> f32 {
    let mut w = 0.0_f32;
    for inter in &node.interactions {
        if inter.kind == EdgeKind::Clash && inter.overlap_a > w {
            w = inter.overlap_a;
        }
    }
    w
}

/// Checks if an atom node participates in a given interaction kind.
pub fn has_interaction_kind(node: &AtomNode, kind: EdgeKind, cross_only: bool) -> bool {
    node.interactions
        .iter()
        .any(|inter| (!cross_only || inter.is_cross_interface) && inter.kind == kind)
}

/// Computes the atom fill color based on the selected visual lens.
pub fn get_atom_fill_color(
    node: &AtomNode,
    meta: &AtomMeta,
    lens: ColorLens,
    bfactor_mode: BFactorMode,
) -> Color32 {
    match lens {
        ColorLens::Interactions => {
            if worst_clash_on_node(node) > 0.05 {
                Color32::from_rgb(242, 20, 20) // Clash red
            } else if has_interaction_kind(node, EdgeKind::SaltBridge, true) {
                Color32::from_rgb(157, 20, 255) // Salt bridge purple
            } else if has_interaction_kind(node, EdgeKind::HBond, true) {
                Color32::from_rgb(37, 99, 235) // H-bond blue
            } else if node.role == InterfaceSide::SideA {
                Color32::from_rgb(14, 165, 233) // Group A sky blue
            } else {
                Color32::from_rgb(251, 146, 60) // Group B orange
            }
        }
        ColorLens::Chemistry => {
            if node.aliphatic_score >= 0.75 {
                Color32::from_rgb(148, 163, 184) // Aliphatic slate
            } else if meta.res_name == "PHE" || meta.res_name == "TYR" || meta.res_name == "TRP" {
                Color32::from_rgb(245, 158, 11) // Aromatic amber
            } else if meta.res_name == "ARG" || meta.res_name == "LYS" || meta.res_name == "HIS" {
                Color32::from_rgb(59, 130, 246) // Positive blue
            } else if meta.res_name == "ASP" || meta.res_name == "GLU" {
                Color32::from_rgb(239, 68, 68) // Negative red
            } else if meta.res_name == "GLY" || meta.res_name == "PRO" {
                Color32::from_rgb(168, 85, 247) // Special purple
            } else {
                Color32::from_rgb(6, 182, 212) // Polar cyan
            }
        }
        ColorLens::ModernCpk => match meta.element {
            smorgasprot_core::Element::C => Color32::from_rgb(71, 85, 105),
            smorgasprot_core::Element::N => Color32::from_rgb(37, 99, 235),
            smorgasprot_core::Element::O => Color32::from_rgb(220, 38, 38),
            smorgasprot_core::Element::S => Color32::from_rgb(217, 119, 6),
            smorgasprot_core::Element::P => Color32::from_rgb(234, 88, 12),
            _ => Color32::from_rgb(100, 116, 139),
        },
        ColorLens::Bfactor => {
            let b = node.b_factor;
            if bfactor_mode == BFactorMode::AlphaFold {
                if b >= 90.0 {
                    Color32::from_rgb(0, 83, 214) // Very high pLDDT blue
                } else if b >= 70.0 {
                    Color32::from_rgb(101, 203, 243) // Confident light blue
                } else if b >= 50.0 {
                    Color32::from_rgb(255, 219, 19) // Low yellow
                } else {
                    Color32::from_rgb(255, 125, 69) // Very low orange
                }
            } else {
                if b <= 30.0 {
                    Color32::from_rgb(0, 83, 214) // Rigid dark blue
                } else if b <= 50.0 {
                    Color32::from_rgb(101, 203, 243) // Ordered light blue
                } else if b <= 70.0 {
                    Color32::from_rgb(255, 219, 19) // Flexible yellow
                } else {
                    Color32::from_rgb(255, 125, 69) // Labile orange
                }
            }
        }
    }
}
