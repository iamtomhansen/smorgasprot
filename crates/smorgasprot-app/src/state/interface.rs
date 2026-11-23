//! State, layer styles, and preset helpers for the 2D/3D Interface Cartography workspace tab.

use super::Tween;
use crate::camera::CameraState;
use crate::style::{
    AtomSizeMode, BFactorMode, ChannelPreset, ChromePalette, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder,
    NodeStrokeColorLens, TerritoryColorLens, TerritoryStrokeLens, VisualChannel,
};
use egui::Color32;
use smorgasprot_core::InterfaceSide;

/// Visual channels and label builder for the atom node layer.
#[derive(Clone, Debug, PartialEq)]
pub struct AtomLayerStyle {
    pub fill: VisualChannel<ColorLens, Color32>,
    pub radius: VisualChannel<AtomSizeMode, f32>,
    pub stroke_color: VisualChannel<NodeStrokeColorLens, Color32>,
    pub stroke_width: VisualChannel<AtomSizeMode, f32>,
    pub labels: LabelBuilder,
}

impl Default for AtomLayerStyle {
    fn default() -> Self {
        let chrome = ChromePalette::default();
        Self {
            fill: VisualChannel::new(ColorLens::Interactions, Color32::from_rgb(86, 180, 233)),
            radius: VisualChannel::new(AtomSizeMode::SqrtBuriedSasa, 1.15),
            stroke_color: VisualChannel::new(NodeStrokeColorLens::DefaultInk, chrome.ink),
            stroke_width: VisualChannel::new(AtomSizeMode::Fixed, 1.0),
            labels: LabelBuilder::default_atom(),
        }
    }
}

impl AtomLayerStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Visual channels and layer toggles for the contact edge layer (intra-side springs & cross-interface arcs).
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeLayerStyle {
    pub color: VisualChannel<EdgeColorLens, Color32>,
    pub width: VisualChannel<EdgeWidthLens, f32>,
    pub show_intra: bool,
    pub show_cross: bool,
}

impl Default for EdgeLayerStyle {
    fn default() -> Self {
        Self {
            color: VisualChannel::new(EdgeColorLens::InteractionKind, Color32::from_rgb(135, 206, 250)),
            width: VisualChannel::new(EdgeWidthLens::InteractionKind, 1.5),
            show_intra: true,
            show_cross: true,
        }
    }
}

impl EdgeLayerStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Visual channels, label builder, and visibility toggle for the residue territory contour layer.
#[derive(Clone, Debug, PartialEq)]
pub struct TerritoryLayerStyle {
    pub visible: bool,
    pub fill: VisualChannel<TerritoryColorLens, Color32>,
    pub stroke_width: VisualChannel<TerritoryStrokeLens, f32>,
    pub labels: LabelBuilder,
}

impl Default for TerritoryLayerStyle {
    fn default() -> Self {
        Self {
            visible: true,
            fill: VisualChannel::new(TerritoryColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            stroke_width: VisualChannel::new(TerritoryStrokeLens::Fixed, 1.2),
            labels: LabelBuilder::default_territory(),
        }
    }
}

impl TerritoryLayerStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Camera, morph, visual-channel layers, and tweened group settings for the Interface workspace tab.
#[derive(Clone, Debug)]
pub struct InterfaceViewSettings {
    pub camera: CameraState,
    /// 3D (`0`) → bas-relief → flat 2D map (`1`).
    pub flatten: Tween,
    /// Overlay (`0`) → open-book butterfly (`1` = 180°).
    pub book: Tween,
    /// Spring-relaxed `(u, v)` instead of the raw tangent-plane projection.
    pub use_relaxed_uv: bool,

    pub atoms: AtomLayerStyle,
    pub edges: EdgeLayerStyle,
    pub territories: TerritoryLayerStyle,

    pub bfactor_mode: BFactorMode,
    pub side_a_alpha: Tween,
    pub side_b_alpha: Tween,
    pub show_backbone: bool,
}

impl Default for InterfaceViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            flatten: Tween::new(1.0),
            book: Tween::new(0.0),
            use_relaxed_uv: true,
            atoms: AtomLayerStyle::default(),
            edges: EdgeLayerStyle::default(),
            territories: TerritoryLayerStyle::default(),
            bfactor_mode: BFactorMode::Crystallographic,
            side_a_alpha: Tween::new(1.0),
            side_b_alpha: Tween::new(0.35),
            show_backbone: true,
        }
    }
}

impl InterfaceViewSettings {
    /// Applies a multi-channel [`ChannelPreset`] across the atom, edge, and territory layers.
    pub fn apply_preset(&mut self, preset: ChannelPreset) {
        self.atoms.reset();
        self.edges.reset();
        self.territories.reset();

        match preset {
            ChannelPreset::Interactions => {}
            ChannelPreset::Mobility => {
                self.atoms.fill.lens = ColorLens::Bfactor;
                self.atoms.radius.lens = AtomSizeMode::Rigidity;
                self.edges.color.lens = EdgeColorLens::EndpointMobility;
                self.edges.width.lens = EdgeWidthLens::EndpointRigidity;
                self.territories.fill.lens = TerritoryColorLens::AvgMobility;
            }
            ChannelPreset::HydrophobicPacking => {
                self.atoms.fill.lens = ColorLens::AaChemistry;
                self.atoms.radius.lens = AtomSizeMode::AliphaticScore;
                self.atoms.stroke_color.lens = NodeStrokeColorLens::UnsatisfiedPolar;
                self.territories.fill.lens = TerritoryColorLens::BuriedSasa;
                self.territories.stroke_width.lens = TerritoryStrokeLens::UnsatisfiedPolar;
            }
            ChannelPreset::StericClashes => {
                self.atoms.fill.lens = ColorLens::ClashSeverity;
                self.atoms.radius.lens = AtomSizeMode::ClashSeverity;
                self.edges.color.lens = EdgeColorLens::Strain2D;
                self.edges.width.lens = EdgeWidthLens::ClashSeverity;
                self.territories.fill.lens = TerritoryColorLens::UnsatisfiedPolar;
            }
            ChannelPreset::ElementalCpk => {
                self.atoms.fill.lens = ColorLens::ModernCpk;
                self.atoms.stroke_color.lens = NodeStrokeColorLens::UnsatisfiedPolar;
                self.territories.fill.lens = TerritoryColorLens::Chemistry;
            }
        }
    }

    /// Resets all visual channels and label builders across every layer to their defaults.
    pub fn reset_all_channels(&mut self) {
        self.atoms.reset();
        self.edges.reset();
        self.territories.reset();
    }

    /// Current displayed opacity for one side of the interface (`0` for unassigned).
    pub fn side_alpha(&self, side: InterfaceSide) -> f32 {
        match side {
            InterfaceSide::SideA => self.side_a_alpha.value,
            InterfaceSide::SideB => self.side_b_alpha.value,
            InterfaceSide::None => 0.0,
        }
    }

    /// Smoothly cross-fades one side to full opacity and dims the other.
    pub fn solo(&mut self, side: InterfaceSide) {
        let (a, b) = match side {
            InterfaceSide::SideA => (1.0, 0.05),
            InterfaceSide::SideB => (0.05, 1.0),
            InterfaceSide::None => return,
        };
        self.side_a_alpha.animate_to(a);
        self.side_b_alpha.animate_to(b);
    }

    /// Smoothly restores both sides, keeping B slightly translucent so A reads as foreground.
    pub fn show_both(&mut self) {
        self.side_a_alpha.animate_to(1.0);
        self.side_b_alpha.animate_to(0.85);
    }

    /// Advances all tweens in the Interface tab; returns `true` while any tween is moving.
    pub fn step_animations(&mut self, dt: f32, rate: f32) -> bool {
        self.flatten.step(dt, rate)
            | self.book.step(dt, rate)
            | self.side_a_alpha.step(dt, rate)
            | self.side_b_alpha.step(dt, rate)
    }

    /// Immediately snaps all Interface tab tweens to their targets.
    pub fn finish_animations(&mut self) {
        self.flatten.finish();
        self.book.finish();
        self.side_a_alpha.finish();
        self.side_b_alpha.finish();
    }
}
