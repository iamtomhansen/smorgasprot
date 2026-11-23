//! Visual channel wrappers, lens enums, scalar metrics, and composable label builders.
//! Each enum implements [`Choice`] so UI controls and popovers can render them generically.

use crate::state::Choice;
use serde::{Deserialize, Serialize};

/// Bindings for one customizable visual channel: active lens, fixed value, and built-in defaults for reset.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq)]
pub struct VisualChannel<L, V> {
    pub lens: L,
    pub fixed: V,
    pub default_lens: L,
    pub default_fixed: V,
}

impl<L: Copy + PartialEq, V: Copy + PartialEq> VisualChannel<L, V> {
    pub const fn new(default_lens: L, default_fixed: V) -> Self {
        Self {
            lens: default_lens,
            fixed: default_fixed,
            default_lens,
            default_fixed,
        }
    }

    /// True if both the active lens and fixed constant equal their defaults.
    pub fn is_default(&self) -> bool {
        self.lens == self.default_lens && self.fixed == self.default_fixed
    }

    /// Restores the channel's active lens and fixed constant to their defaults.
    pub fn reset(&mut self) {
        self.lens = self.default_lens;
        self.fixed = self.default_fixed;
    }
}

/// Shared continuous 1D biophysical metrics evaluated across color, sizing, and label channels.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScalarMetric {
    BuriedSasa,
    Mobility,
    Rigidity,
    AliphaticScore,
    ClashSeverity,
}

/// Data projection assigned to atom node fill color.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ColorLens {
    Fixed,
    Off,
    #[default]
    Interactions,
    AaChemistry,
    ModernCpk,
    InterfaceSide,
    Bfactor,
    BuriedSasa,
    AliphaticScore,
    ClashSeverity,
}

impl ColorLens {
    /// Returns the underlying [`ScalarMetric`] if this lens is a continuous 1D metric.
    pub const fn scalar_metric(self) -> Option<ScalarMetric> {
        match self {
            Self::BuriedSasa => Some(ScalarMetric::BuriedSasa),
            Self::Bfactor => Some(ScalarMetric::Mobility),
            Self::AliphaticScore => Some(ScalarMetric::AliphaticScore),
            Self::ClashSeverity => Some(ScalarMetric::ClashSeverity),
            _ => None,
        }
    }
}

impl Choice for ColorLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::Interactions,
        Self::AaChemistry,
        Self::ModernCpk,
        Self::InterfaceSide,
        Self::Bfactor,
        Self::BuriedSasa,
        Self::AliphaticScore,
        Self::ClashSeverity,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Color",
            Self::Off => "Off (No Fill)",
            Self::Interactions => "Specific Interactions (H-Bonds / Salt Bridges / Clashes)",
            Self::AaChemistry => "AA Chemistry (Aliphatic / Polar / Charge / Aromatic)",
            Self::ModernCpk => "Modern CPK (Elemental Core Colors)",
            Self::InterfaceSide => "Interface Group (Side A vs. Side B)",
            Self::Bfactor => "B-Factor / pLDDT Mobility (Continuous)",
            Self::BuriedSasa => "Buried Interface Area ΔSASA (Continuous)",
            Self::AliphaticScore => "Aliphatic Burial Score (Continuous)",
            Self::ClashSeverity => "Steric Clash Overlap (Continuous)",
        }
    }
}

/// Data projection assigned to atom node border/stroke color.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum NodeStrokeColorLens {
    Fixed,
    Off,
    #[default]
    DefaultInk,
    UnsatisfiedPolar,
    Interactions,
    Chemistry,
    ModernCpk,
    InterfaceSide,
    Bfactor,
}

impl Choice for NodeStrokeColorLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::DefaultInk,
        Self::UnsatisfiedPolar,
        Self::Interactions,
        Self::Chemistry,
        Self::ModernCpk,
        Self::InterfaceSide,
        Self::Bfactor,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Color",
            Self::Off => "Off (No Border)",
            Self::DefaultInk => "Default Ink Outline",
            Self::UnsatisfiedPolar => "Highlight Unsatisfied Buried Polars",
            Self::Interactions => "Specific Interactions",
            Self::Chemistry => "AA Chemistry",
            Self::ModernCpk => "Modern CPK Element",
            Self::InterfaceSide => "Interface Group (Side A / B)",
            Self::Bfactor => "B-Factor / pLDDT Mobility",
        }
    }
}

/// Data projection assigned to whole-chain 3D representations (molecular surfaces and Cα cartoon ribbons).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ChainColorLens {
    Fixed,
    Off,
    #[default]
    InterfaceSide,
    ChainIndex,
}

impl Choice for ChainColorLens {
    const ALL: &'static [Self] = &[Self::Fixed, Self::Off, Self::InterfaceSide, Self::ChainIndex];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Color",
            Self::Off => "Off (Hidden)",
            Self::InterfaceSide => "Interface Group (Side A / Side B / Unassigned)",
            Self::ChainIndex => "By Polypeptide Chain (Categorical Palette)",
        }
    }
}

/// Scalar data projection assigned to atom node radius or node stroke width.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum AtomSizeMode {
    Fixed,
    Off,
    #[default]
    SqrtBuriedSasa,
    ClashSeverity,
    Rigidity,
    Mobility,
    AliphaticScore,
}

impl AtomSizeMode {
    /// Returns the underlying [`ScalarMetric`] if this mode is data-driven.
    pub const fn scalar_metric(self) -> Option<ScalarMetric> {
        match self {
            Self::SqrtBuriedSasa => Some(ScalarMetric::BuriedSasa),
            Self::ClashSeverity => Some(ScalarMetric::ClashSeverity),
            Self::Rigidity => Some(ScalarMetric::Rigidity),
            Self::Mobility => Some(ScalarMetric::Mobility),
            Self::AliphaticScore => Some(ScalarMetric::AliphaticScore),
            Self::Fixed | Self::Off => None,
        }
    }
}

impl Choice for AtomSizeMode {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::SqrtBuriedSasa,
        Self::ClashSeverity,
        Self::Rigidity,
        Self::Mobility,
        Self::AliphaticScore,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Value",
            Self::Off => "Off",
            Self::SqrtBuriedSasa => "Buried Interface Area (sqrt ΔSASA)",
            Self::ClashSeverity => "Steric Clash Severity",
            Self::Rigidity => "Structural Rigidity / Confidence (1 - B-Factor / pLDDT)",
            Self::Mobility => "Crystallographic Mobility (B-Factor / 1 - pLDDT)",
            Self::AliphaticScore => "Aliphatic Burial Score",
        }
    }
}

/// Data projection assigned to contact edge color.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum EdgeColorLens {
    Fixed,
    Off,
    #[default]
    InteractionKind,
    Strain2D,
    EndpointMobility,
    Distance3D,
}

impl Choice for EdgeColorLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::InteractionKind,
        Self::Strain2D,
        Self::EndpointMobility,
        Self::Distance3D,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Color",
            Self::Off => "Off (No Edges)",
            Self::InteractionKind => "Interaction Kind (Clash / Salt Bridge / H-Bond / vdW)",
            Self::Strain2D => "2D Flattening Strain (Compressed ↔ Stretched)",
            Self::EndpointMobility => "Average Endpoint B-Factor / pLDDT",
            Self::Distance3D => "3D Contact Distance",
        }
    }
}

/// Scalar data projection assigned to contact edge stroke width.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum EdgeWidthLens {
    Fixed,
    Off,
    #[default]
    InteractionKind,
    EndpointRigidity,
    Proximity3D,
    ClashSeverity,
    Strain2D,
}

impl Choice for EdgeWidthLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::InteractionKind,
        Self::EndpointRigidity,
        Self::Proximity3D,
        Self::ClashSeverity,
        Self::Strain2D,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Width",
            Self::Off => "Off (No Edges)",
            Self::InteractionKind => "By Interaction Kind",
            Self::EndpointRigidity => "Average Endpoint Rigidity / Confidence",
            Self::Proximity3D => "3D Contact Proximity (Closer = Wider)",
            Self::ClashSeverity => "Steric Clash Overlap",
            Self::Strain2D => "2D Metric Strain Magnitude",
        }
    }
}

/// Data projection assigned to residue territory polygon fill color.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum TerritoryColorLens {
    Fixed,
    Off,
    #[default]
    InterfaceSide,
    Chemistry,
    BuriedSasa,
    AvgMobility,
    UnsatisfiedPolar,
}

impl TerritoryColorLens {
    /// Returns the underlying [`ScalarMetric`] if this lens is a continuous 1D metric.
    pub const fn scalar_metric(self) -> Option<ScalarMetric> {
        match self {
            Self::BuriedSasa => Some(ScalarMetric::BuriedSasa),
            Self::AvgMobility => Some(ScalarMetric::Mobility),
            _ => None,
        }
    }
}

impl Choice for TerritoryColorLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::InterfaceSide,
        Self::Chemistry,
        Self::BuriedSasa,
        Self::AvgMobility,
        Self::UnsatisfiedPolar,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Color",
            Self::Off => "Off (No Territory Fill)",
            Self::InterfaceSide => "Interface Group (Side A vs. Side B)",
            Self::Chemistry => "Residue AA Chemistry",
            Self::BuriedSasa => "Total Residue Buried ΔSASA",
            Self::AvgMobility => "Average Residue B-Factor / pLDDT",
            Self::UnsatisfiedPolar => "Unsatisfied Buried Polar Status",
        }
    }
}

/// Scalar data projection assigned to residue territory border stroke width.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum TerritoryStrokeLens {
    #[default]
    Fixed,
    Off,
    BuriedSasa,
    UnsatisfiedPolar,
    AvgRigidity,
}

impl TerritoryStrokeLens {
    /// Returns the underlying [`ScalarMetric`] if this lens is a continuous 1D metric.
    pub const fn scalar_metric(self) -> Option<ScalarMetric> {
        match self {
            Self::BuriedSasa => Some(ScalarMetric::BuriedSasa),
            Self::AvgRigidity => Some(ScalarMetric::Rigidity),
            _ => None,
        }
    }
}

impl Choice for TerritoryStrokeLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::BuriedSasa,
        Self::UnsatisfiedPolar,
        Self::AvgRigidity,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed Border Width",
            Self::Off => "Off (No Territory Border)",
            Self::BuriedSasa => "Total Residue Buried ΔSASA",
            Self::UnsatisfiedPolar => "Highlight Unsatisfied Buried Polars",
            Self::AvgRigidity => "Average Residue Rigidity / Confidence",
        }
    }
}

/// Format used for residue names in atom and territory labels.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ResidueNameStyle {
    Off,
    OneLetter,
    #[default]
    ThreeLetter,
    FullName,
}

impl Choice for ResidueNameStyle {
    const ALL: &'static [Self] = &[Self::Off, Self::OneLetter, Self::ThreeLetter, Self::FullName];

    fn label(self) -> &'static str {
        match self {
            Self::Off => "None (Hide Residue Name)",
            Self::OneLetter => "1-Letter Code (E, A, ...)",
            Self::ThreeLetter => "3-Letter Code (Glu, Ala, ...)",
            Self::FullName => "Full Name (Glutamate, Cefuroxime, ...)",
        }
    }
}

/// Numeric data channel appended to an atom or territory label.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum LabelValueChannel {
    #[default]
    Off,
    BuriedSasa,
    Bfactor,
    AliphaticScore,
    ClashSeverity,
    ContactCount,
}

impl LabelValueChannel {
    /// Returns the underlying [`ScalarMetric`] if this channel is a continuous 1D metric.
    pub const fn scalar_metric(self) -> Option<ScalarMetric> {
        match self {
            Self::BuriedSasa => Some(ScalarMetric::BuriedSasa),
            Self::Bfactor => Some(ScalarMetric::Mobility),
            Self::AliphaticScore => Some(ScalarMetric::AliphaticScore),
            Self::ClashSeverity => Some(ScalarMetric::ClashSeverity),
            Self::Off | Self::ContactCount => None,
        }
    }
}

impl Choice for LabelValueChannel {
    const ALL: &'static [Self] = &[
        Self::Off,
        Self::BuriedSasa,
        Self::Bfactor,
        Self::AliphaticScore,
        Self::ClashSeverity,
        Self::ContactCount,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Off => "Off (No Extra Metric)",
            Self::BuriedSasa => "Buried ΔSASA (Å²)",
            Self::Bfactor => "B-Factor / pLDDT",
            Self::AliphaticScore => "Aliphatic Burial Score",
            Self::ClashSeverity => "Worst Steric Clash Overlap (Å)",
            Self::ContactCount => "Cross-Interface Contact Count",
        }
    }
}

/// Composable builder for atom node or residue territory text labels.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LabelBuilder {
    pub enabled: bool,
    pub res_name_style: ResidueNameStyle,
    pub show_res_num: bool,
    pub show_chain: bool,
    pub show_atom_name: bool,
    pub show_sec_struct: bool,
    pub value_channel: LabelValueChannel,
}

impl LabelBuilder {
    /// Default label configuration for residue territories (3-letter name + residue number).
    pub fn default_territory() -> Self {
        Self {
            enabled: true,
            res_name_style: ResidueNameStyle::ThreeLetter,
            show_res_num: true,
            show_chain: false,
            show_atom_name: false,
            show_sec_struct: false,
            value_channel: LabelValueChannel::Off,
        }
    }

    /// Default label configuration for atom nodes (disabled by default to avoid clutter; atom name when enabled).
    pub fn default_atom() -> Self {
        Self {
            enabled: false,
            res_name_style: ResidueNameStyle::Off,
            show_res_num: false,
            show_chain: false,
            show_atom_name: true,
            show_sec_struct: false,
            value_channel: LabelValueChannel::Off,
        }
    }
}

/// Curated multi-channel visual presets for rapid 1-click exploration in the Interface tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChannelPreset {
    Interactions,
    Mobility,
    HydrophobicPacking,
    StericClashes,
    ElementalCpk,
}

impl ChannelPreset {
    pub const ALL: &'static [Self] = &[
        Self::Interactions,
        Self::Mobility,
        Self::HydrophobicPacking,
        Self::StericClashes,
        Self::ElementalCpk,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Interactions => "Interactions",
            Self::Mobility => "Mobility / pLDDT",
            Self::HydrophobicPacking => "Hydrophobic Core",
            Self::StericClashes => "Clashes & Strain",
            Self::ElementalCpk => "CPK Elements",
        }
    }
}

/// How the per-atom B-factor column is interpreted (AlphaFold stores pLDDT there, inverted).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BFactorMode {
    #[default]
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
