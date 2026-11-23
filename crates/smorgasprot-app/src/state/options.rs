//! User-selectable display and visual-channel encoding options (enums) shown in combo boxes.
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

/// Data projection assigned to atom node fill color.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ColorLens {
    Fixed,
    Off,
    #[default]
    Interactions,
    Chemistry,
    ModernCpk,
    InterfaceSide,
    Bfactor,
    BuriedSasa,
    AliphaticScore,
    ClashSeverity,
}

impl Choice for ColorLens {
    const ALL: &'static [Self] = &[
        Self::Fixed,
        Self::Off,
        Self::Interactions,
        Self::Chemistry,
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
            Self::Chemistry => "AA Chemistry (Aliphatic / Polar / Charge / Aromatic)",
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
