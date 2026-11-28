//! State, layer styles, and preset helpers for the 2D/3D Interface Cartography workspace tab.

use super::{BoundaryLayerStyle, MeshShading, PresentationStyle, RibbonStyle, SphereQuality, SurfaceCulling, Tween};
use crate::camera::CameraState;
use crate::style::{
    AtomSizeMode, BFactorMode, ChannelPreset, ChromePalette, ColorLens, ColorMap, EdgeColorLens, EdgeWidthLens,
    LabelBuilder, NodeStrokeColorLens, Palette, TerritoryColorLens, TerritoryStrokeLens, VisualChannel,
};
use egui::Color32;
use smorgasprot_core::InterfaceSide;

/// Coloring mode for the morphed 3D ↔ 2D interface sheet in the Sandwich view.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum SheetColorMode {
    /// Palette diverging strain ramp: blue (compressed) → white (isometric) → red (stretched).
    #[default]
    StrainBlueWhiteRed,
    /// Colorblind-safe diverging strain ramp: teal (compressed) → white → orange (stretched).
    StrainTealWhiteOrange,
    /// Diverging strain ramp: purple (compressed) → white → green (stretched).
    StrainPurpleWhiteGreen,
    /// Uniform user-picked color (no strain encoding).
    Fixed,
}

impl SheetColorMode {
    pub const ALL: &'static [Self] = &[
        Self::StrainBlueWhiteRed,
        Self::StrainTealWhiteOrange,
        Self::StrainPurpleWhiteGreen,
        Self::Fixed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::StrainBlueWhiteRed => "Strain (Blue–White–Red)",
            Self::StrainTealWhiteOrange => "Strain (Teal–White–Orange)",
            Self::StrainPurpleWhiteGreen => "Strain (Purple–White–Green)",
            Self::Fixed => "Fixed color",
        }
    }

    pub fn legend_text(self) -> &'static str {
        match self {
            Self::StrainBlueWhiteRed => "Blue = compressed · White = isometric · Red = stretched (±35%)",
            Self::StrainTealWhiteOrange => "Teal = compressed · White = isometric · Orange = stretched (±35%)",
            Self::StrainPurpleWhiteGreen => "Purple = compressed · White = isometric · Green = stretched (±35%)",
            Self::Fixed => "Interface sheet (fixed color)",
        }
    }

    /// Evaluates the vertex color for relative linear `strain` (saturating at ±35%).
    pub fn sample(self, pal: &Palette, strain: f32, fixed: Color32) -> Color32 {
        const SATURATION: f32 = 0.35;
        let t = 0.5 + 0.5 * (strain / SATURATION).clamp(-1.0, 1.0);
        match self {
            Self::StrainBlueWhiteRed => pal.strain.sample(t),
            Self::StrainTealWhiteOrange => ColorMap::diverging(
                Color32::from_rgb(13, 148, 136),
                Color32::WHITE,
                Color32::from_rgb(234, 88, 12),
            )
            .sample(t),
            Self::StrainPurpleWhiteGreen => ColorMap::diverging(
                Color32::from_rgb(126, 34, 206),
                Color32::WHITE,
                Color32::from_rgb(22, 163, 74),
            )
            .sample(t),
            Self::Fixed => fixed,
        }
    }
}

/// Visual channels, presentation, and label builder for the atom node layer.
#[derive(Clone, Debug, PartialEq)]
pub struct AtomLayerStyle {
    pub visible: bool,
    pub fill: VisualChannel<ColorLens, Color32>,
    pub radius: VisualChannel<AtomSizeMode, f32>,
    pub stroke_color: VisualChannel<NodeStrokeColorLens, Color32>,
    pub stroke_width: VisualChannel<AtomSizeMode, f32>,
    pub presentation: PresentationStyle<SphereQuality>,
    pub labels: LabelBuilder,
}

impl Default for AtomLayerStyle {
    fn default() -> Self {
        let chrome = ChromePalette::default();
        Self {
            visible: true,
            fill: VisualChannel::new(ColorLens::Interactions, Color32::from_rgb(86, 180, 233)),
            radius: VisualChannel::new(AtomSizeMode::SqrtBuriedSasa, 1.15),
            stroke_color: VisualChannel::new(NodeStrokeColorLens::DefaultInk, chrome.ink),
            stroke_width: VisualChannel::new(AtomSizeMode::Fixed, 1.0),
            presentation: Self::default_presentation(),
            labels: LabelBuilder::default_atom(),
        }
    }
}

impl AtomLayerStyle {
    pub fn default_presentation() -> PresentationStyle<SphereQuality> {
        PresentationStyle {
            quality: SphereQuality::Flat2D,
            shading: MeshShading::Smooth,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn channels_default(&self) -> bool {
        let d = Self::default();
        self.fill == d.fill
            && self.radius == d.radius
            && self.stroke_color == d.stroke_color
            && self.stroke_width == d.stroke_width
    }

    pub fn reset_channels(&mut self) {
        let d = Self::default();
        self.fill = d.fill;
        self.radius = d.radius;
        self.stroke_color = d.stroke_color;
        self.stroke_width = d.stroke_width;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Visual channels, presentation, and filter toggles for the contact edge layer.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeLayerStyle {
    pub visible: bool,
    pub color: VisualChannel<EdgeColorLens, Color32>,
    pub width: VisualChannel<EdgeWidthLens, f32>,
    pub presentation: PresentationStyle<RibbonStyle>,
    pub show_intra: bool,
    pub show_cross: bool,
    pub show_covalent: bool,
    pub show_hbond: bool,
    pub show_salt_bridge: bool,
    pub show_clash: bool,
    pub show_vdw: bool,
}

impl Default for EdgeLayerStyle {
    fn default() -> Self {
        Self {
            visible: true,
            color: VisualChannel::new(EdgeColorLens::InteractionKind, Color32::from_rgb(135, 206, 250)),
            width: VisualChannel::new(EdgeWidthLens::InteractionKind, 1.5),
            presentation: Self::default_presentation(),
            show_intra: true,
            show_cross: true,
            show_covalent: true,
            show_hbond: true,
            show_salt_bridge: true,
            show_clash: true,
            show_vdw: true,
        }
    }
}

impl EdgeLayerStyle {
    pub fn default_presentation() -> PresentationStyle<RibbonStyle> {
        PresentationStyle {
            quality: RibbonStyle::Capsule2D,
            shading: MeshShading::Smooth,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn channels_default(&self) -> bool {
        let d = Self::default();
        self.color == d.color && self.width == d.width
    }

    pub fn filters_default(&self) -> bool {
        let d = Self::default();
        self.show_intra == d.show_intra
            && self.show_cross == d.show_cross
            && self.show_covalent == d.show_covalent
            && self.show_hbond == d.show_hbond
            && self.show_salt_bridge == d.show_salt_bridge
            && self.show_clash == d.show_clash
            && self.show_vdw == d.show_vdw
    }

    pub fn reset_channels(&mut self) {
        let d = Self::default();
        self.color = d.color;
        self.width = d.width;
    }

    pub fn reset_filters(&mut self) {
        let d = Self::default();
        self.show_intra = d.show_intra;
        self.show_cross = d.show_cross;
        self.show_covalent = d.show_covalent;
        self.show_hbond = d.show_hbond;
        self.show_salt_bridge = d.show_salt_bridge;
        self.show_clash = d.show_clash;
        self.show_vdw = d.show_vdw;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Visual channels, presentation, label builder, and visibility toggle for residue territories.
#[derive(Clone, Debug, PartialEq)]
pub struct TerritoryLayerStyle {
    pub visible: bool,
    pub fill: VisualChannel<TerritoryColorLens, Color32>,
    pub stroke_width: VisualChannel<TerritoryStrokeLens, f32>,
    pub presentation: PresentationStyle<()>,
    pub labels: LabelBuilder,
}

impl Default for TerritoryLayerStyle {
    fn default() -> Self {
        Self {
            visible: true,
            fill: VisualChannel::new(TerritoryColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            stroke_width: VisualChannel::new(TerritoryStrokeLens::Fixed, 1.2),
            presentation: Self::default_presentation(),
            labels: LabelBuilder::default_territory(),
        }
    }
}

impl TerritoryLayerStyle {
    pub fn default_presentation() -> PresentationStyle<()> {
        let mut p = PresentationStyle {
            quality: (),
            shading: MeshShading::Unlit,
            culling: SurfaceCulling::None,
            ..PresentationStyle::default()
        };
        p.alphas.fill = 0.15;
        p
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Camera, morph, visual-channel layers, and tweened group settings for the Cartography (Sandwich) workspace tab.
#[derive(Clone, Debug)]
pub struct CartographyViewSettings {
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
    /// Morphed 3D ↔ 2D interface sheet visibility, opacity, and mesh presentation.
    pub sheet: BoundaryLayerStyle,
    /// Coloring mode for the morphed interface sheet (diverging strain ramp or fixed color).
    pub sheet_color: SheetColorMode,
    /// Uniform color used when `sheet_color == SheetColorMode::Fixed`.
    pub sheet_fixed_color: Color32,
    /// 3D contour draping blend: `0.0` = through atomic centers, `0.99` = 99% onto interface sheet.
    pub contour_sheet_blend: f32,
    /// Which side's territories/labels paint on top of the other — a pure presentation choice,
    /// independent of `InterfacePartition`'s actual chain-to-side assignment.
    pub side_a_on_top: bool,
}

impl Default for CartographyViewSettings {
    fn default() -> Self {
        let mut sheet = BoundaryLayerStyle::new(false);
        sheet.alpha = Tween::new(0.75);
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
            sheet,
            sheet_color: SheetColorMode::default(),
            sheet_fixed_color: Color32::from_rgb(148, 163, 184),
            contour_sheet_blend: 0.90,
            side_a_on_top: true,
        }
    }
}

impl CartographyViewSettings {
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

    /// Back-to-front paint order for side-layered geometry (territories, labels): the first
    /// element is painted first (bottom), so the second one reads on top.
    pub fn side_paint_order(&self) -> [InterfaceSide; 2] {
        if self.side_a_on_top {
            [InterfaceSide::SideB, InterfaceSide::SideA]
        } else {
            [InterfaceSide::SideA, InterfaceSide::SideB]
        }
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
            | self.sheet.alpha.step(dt, rate)
    }

    /// Immediately snaps all Interface tab tweens to their targets.
    pub fn finish_animations(&mut self) {
        self.flatten.finish();
        self.book.finish();
        self.side_a_alpha.finish();
        self.side_b_alpha.finish();
        self.sheet.alpha.finish();
    }
}
