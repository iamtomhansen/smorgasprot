//! User-selectable display options (enums) shown in combo boxes.
//! Each implements [`Choice`] so panels can render them with one generic widget.

use serde::{Deserialize, Serialize};

/// A closed set of labelled options, rendered by `panels::widgets::choice_combo`.
pub trait Choice: Copy + PartialEq + 'static {
    /// Every option in display order.
    const ALL: &'static [Self];
    /// Human-readable label for menus.
    fn label(self) -> &'static str;
}

/// Top-level workspace tab (Blender-style).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    /// 3D assembly setup: surfaces, cartoon, explode.
    Complex,
    /// Unwrapped 2D/3D interface cartography.
    Interface,
}

/// Which derived interface geometry is drawn in the Complex tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum InterfaceDisplayMode {
    SmoothedSdf,
    ContactEnvelope,
    NegativeSpace,
    Off,
}

impl Choice for InterfaceDisplayMode {
    const ALL: &'static [Self] = &[Self::SmoothedSdf, Self::ContactEnvelope, Self::NegativeSpace, Self::Off];

    fn label(self) -> &'static str {
        match self {
            Self::SmoothedSdf => "Smoothed SDF Sheet",
            Self::ContactEnvelope => "2-Sided Contact Envelope",
            Self::NegativeSpace => "Negative Space (Sub-Solvent Voids)",
            Self::Off => "Off",
        }
    }
}

/// Atom fill-color encoding in the Interface tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ColorLens {
    Interactions,
    Chemistry,
    ModernCpk,
    Bfactor,
}

impl Choice for ColorLens {
    const ALL: &'static [Self] = &[Self::Interactions, Self::Chemistry, Self::ModernCpk, Self::Bfactor];

    fn label(self) -> &'static str {
        match self {
            Self::Interactions => "1. Specific Interactions (H-Bonds / Salt Bridges / Clashes)",
            Self::Chemistry => "2. Chemical Category (Aliphatic / Polar / Charge / Aromatic)",
            Self::ModernCpk => "3. Modern CPK (Elemental Core Colors)",
            Self::Bfactor => "4. B-Factor & Crystallographic Mobility / pLDDT",
        }
    }
}

/// How the per-atom B-factor column is interpreted (AlphaFold stores pLDDT there, inverted).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum BFactorMode {
    Crystallographic,
    AlphaFold,
}

impl Choice for BFactorMode {
    const ALL: &'static [Self] = &[Self::Crystallographic, Self::AlphaFold];

    fn label(self) -> &'static str {
        match self {
            Self::Crystallographic => "Crystallographic B-Factor (Rigid Blue <= 30 -> Labile Red > 70)",
            Self::AlphaFold => "AlphaFold pLDDT (High Blue > 90 -> Low Red < 50)",
        }
    }
}

/// Atom radius encoding in the Interface tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum AtomSizeMode {
    SqrtBuriedSasa,
    Uniform,
    ClashSeverity,
}

impl Choice for AtomSizeMode {
    const ALL: &'static [Self] = &[Self::SqrtBuriedSasa, Self::Uniform, Self::ClashSeverity];

    fn label(self) -> &'static str {
        match self {
            Self::SqrtBuriedSasa => "Buried Interface Area (sqrt ΔSASA)",
            Self::Uniform => "Uniform Size",
            Self::ClashSeverity => "Steric Clash Severity",
        }
    }
}
