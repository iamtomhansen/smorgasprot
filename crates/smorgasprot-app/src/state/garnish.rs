//! PyMOL/ChimeraX-style Garnish presentation settings, carried over into Slice mode.

use super::presentation::{
    BallStickQuality, BallStickScope, BondColorMode, CartoonQuality, MeshShading, PresentationStyle, RibbonStyle,
    SphereQuality, SurfaceCulling,
};
use super::Tween;
use crate::camera::CameraState;
use crate::style::{AtomSizeMode, BFactorMode, ChainColorLens, ColorLens, NodeStrokeColorLens, VisualChannel};
use egui::Color32;

/// Molecular surface representation style & channels.
#[derive(Clone, Debug)]
pub struct SurfaceRepStyle {
    pub color: VisualChannel<ChainColorLens, Color32>,
    pub presentation: PresentationStyle<()>,
}

impl Default for SurfaceRepStyle {
    fn default() -> Self {
        Self {
            color: VisualChannel::new(ChainColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            presentation: Self::default_presentation(),
        }
    }
}

impl SurfaceRepStyle {
    pub fn default_presentation() -> PresentationStyle<()> {
        PresentationStyle {
            shading: MeshShading::Flat,
            ..PresentationStyle::default()
        }
    }
}

/// Cα backbone ribbon representation style & channels.
#[derive(Clone, Debug)]
pub struct RibbonRepStyle {
    pub color: VisualChannel<ChainColorLens, Color32>,
    pub width: VisualChannel<AtomSizeMode, f32>,
    pub width_scale: f32,
    pub presentation: PresentationStyle<RibbonStyle>,
}

impl Default for RibbonRepStyle {
    fn default() -> Self {
        Self {
            color: VisualChannel::new(ChainColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            width: VisualChannel::new(AtomSizeMode::Fixed, 0.28),
            width_scale: 1.0,
            presentation: Self::default_presentation(),
        }
    }
}

impl RibbonRepStyle {
    pub fn default_presentation() -> PresentationStyle<RibbonStyle> {
        PresentationStyle {
            quality: RibbonStyle::Capsule2D,
            shading: MeshShading::Smooth,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }

    pub fn is_default_channels(&self) -> bool {
        self.color.is_default() && self.width.is_default() && (self.width_scale - 1.0).abs() < 1e-4
    }

    pub fn reset_channels(&mut self) {
        self.color.reset();
        self.width.reset();
        self.width_scale = 1.0;
    }
}

/// Geometric parameters for DSSP Helix–Sheet–Loop protein cartoons.
#[derive(Clone, Debug, PartialEq)]
pub struct CartoonParams {
    /// Blend toward a regularized helix axis (`0.0` = raw Cα spiral, `1.0` = ideal helix).
    pub helix_smoothing: f32,
    /// Radius of helix cylinders / helical ribbons (Å).
    pub helix_radius_a: f32,
    /// Taper an arrowhead tip at the C-terminus of detailed helices.
    pub helix_arrow: bool,
    /// Width of β-sheet ribbons and schematic boxes (Å).
    pub sheet_width_a: f32,
    /// Thickness of β-sheet ribbons and schematic boxes (Å).
    pub sheet_thickness_a: f32,
    /// Preserve alternating Cα pleats in β-sheets instead of smoothing into a flat ribbon.
    pub sheet_zigzag: bool,
    /// Arrowhead width multiplier at the C-terminus of detailed β-strands.
    pub sheet_arrow_scale: f32,
    /// Radius of spline-smoothed `Loop` tubes (Å).
    pub loop_radius_a: f32,
    /// Catmull-Rom spline subdivisions per residue.
    pub spline_subdiv: usize,
}

impl Default for CartoonParams {
    fn default() -> Self {
        Self {
            helix_smoothing: 0.65,
            helix_radius_a: 1.45,
            helix_arrow: true,
            sheet_width_a: 2.0,
            sheet_thickness_a: 0.42,
            sheet_zigzag: false,
            sheet_arrow_scale: 1.50,
            loop_radius_a: 0.22,
            spline_subdiv: 3,
        }
    }
}

/// DSSP Helix–Sheet–Loop protein cartoon representation style & channels.
#[derive(Clone, Debug)]
pub struct CartoonRepStyle {
    pub color: VisualChannel<ChainColorLens, Color32>,
    pub params: CartoonParams,
    pub presentation: PresentationStyle<CartoonQuality>,
}

impl Default for CartoonRepStyle {
    fn default() -> Self {
        Self {
            color: VisualChannel::new(ChainColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            params: CartoonParams::default(),
            presentation: Self::default_presentation(),
        }
    }
}

impl CartoonRepStyle {
    pub fn default_presentation() -> PresentationStyle<CartoonQuality> {
        PresentationStyle {
            quality: CartoonQuality::Detailed,
            shading: MeshShading::Smooth,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }
}

/// Atom sphere representation style & channels (shared by Complex all-atom spheres and Interface spheres).
#[derive(Clone, Debug)]
pub struct SphereRepStyle {
    pub fill: VisualChannel<ColorLens, Color32>,
    pub radius: VisualChannel<AtomSizeMode, f32>,
    pub stroke_color: VisualChannel<NodeStrokeColorLens, Color32>,
    pub stroke_width: VisualChannel<AtomSizeMode, f32>,
    pub presentation: PresentationStyle<SphereQuality>,
}

impl Default for SphereRepStyle {
    fn default() -> Self {
        Self {
            fill: VisualChannel::new(ColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            radius: VisualChannel::new(AtomSizeMode::Fixed, 1.0),
            stroke_color: VisualChannel::new(NodeStrokeColorLens::DefaultInk, super::presentation::DEFAULT_BORDER_COLOR),
            stroke_width: VisualChannel::new(AtomSizeMode::Fixed, 1.0),
            presentation: Self::default_presentation(),
        }
    }
}

impl SphereRepStyle {
    pub fn default_presentation() -> PresentationStyle<SphereQuality> {
        PresentationStyle {
            quality: SphereQuality::Billboard3D,
            shading: MeshShading::Flat,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }

    pub fn is_default_channels(&self) -> bool {
        self.fill.is_default()
            && self.radius.is_default()
            && self.stroke_color.is_default()
            && self.stroke_width.is_default()
    }

    pub fn reset_channels(&mut self) {
        self.fill.reset();
        self.radius.reset();
        self.stroke_color.reset();
        self.stroke_width.reset();
    }
}

/// Geometry & bond-order styling parameters for Ball-and-Stick representations.
#[derive(Clone, Debug, PartialEq)]
pub struct BallStickParams {
    pub scope: BallStickScope,
    /// Bond cylinder radius / 2D capsule half-width (Å).
    pub bond_radius_a: f32,
    pub bond_color_mode: BondColorMode,
    /// Fixed bond color used when `bond_color_mode == BondColorMode::Uniform`.
    pub bond_color: Color32,
    /// Draw parallel offset cylinders/capsules for `BondOrder::Double` / `Triple`.
    pub show_double_bonds: bool,
    /// Draw aromatic ring inner bond indicators for `BondOrder::Aromatic`.
    pub show_aromatic_rings: bool,
}

impl Default for BallStickParams {
    fn default() -> Self {
        Self {
            scope: BallStickScope::AllAtoms,
            bond_radius_a: 0.14,
            bond_color_mode: BondColorMode::SplitMidpoint,
            bond_color: Color32::from_rgb(148, 163, 184),
            show_double_bonds: true,
            show_aromatic_rings: true,
        }
    }
}

/// Covalent Ball-and-Stick representation style & channels.
#[derive(Clone, Debug)]
pub struct BallStickRepStyle {
    pub atom_color: VisualChannel<ColorLens, Color32>,
    pub atom_scale: VisualChannel<AtomSizeMode, f32>,
    pub params: BallStickParams,
    pub presentation: PresentationStyle<BallStickQuality>,
}

impl Default for BallStickRepStyle {
    fn default() -> Self {
        Self {
            atom_color: VisualChannel::new(ColorLens::ModernCpk, Color32::from_rgb(86, 180, 233)),
            atom_scale: VisualChannel::new(AtomSizeMode::Fixed, 0.28),
            params: BallStickParams::default(),
            presentation: Self::default_presentation(),
        }
    }
}

impl BallStickRepStyle {
    pub fn default_presentation() -> PresentationStyle<BallStickQuality> {
        PresentationStyle {
            quality: BallStickQuality::Impostor3D,
            shading: MeshShading::Smooth,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }

    pub fn is_default_channels(&self) -> bool {
        self.atom_color.is_default() && self.atom_scale.is_default()
    }

    pub fn reset_channels(&mut self) {
        self.atom_color.reset();
        self.atom_scale.reset();
    }
}

/// State for the Garnish ("3. Garnish") presentation calibration workspace.
#[derive(Clone, Debug)]
pub struct GarnishViewSettings {
    pub camera: CameraState,
    pub sync_with_slice: bool,
    pub show_interface: bool,
    pub surface_visible: bool,
    pub surface_alpha: Tween,
    pub ribbon_visible: bool,
    pub ribbon_alpha: Tween,
    pub cartoon_visible: bool,
    pub cartoon_alpha: Tween,
    pub ball_stick_visible: bool,
    pub ball_stick_alpha: Tween,
    pub spheres_visible: bool,
    pub spheres_alpha: Tween,
    pub bfactor_mode: BFactorMode,
    pub surface: SurfaceRepStyle,
    pub ribbon: RibbonRepStyle,
    pub cartoon: CartoonRepStyle,
    pub ball_stick: BallStickRepStyle,
    pub spheres: SphereRepStyle,
}

impl Default for GarnishViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            sync_with_slice: true,
            show_interface: false,
            surface_visible: true,
            surface_alpha: Tween::new(0.82),
            ribbon_visible: true,
            ribbon_alpha: Tween::new(0.70),
            cartoon_visible: false,
            cartoon_alpha: Tween::new(0.92),
            ball_stick_visible: false,
            ball_stick_alpha: Tween::new(1.0),
            spheres_visible: false,
            spheres_alpha: Tween::new(0.90),
            bfactor_mode: BFactorMode::default(),
            surface: SurfaceRepStyle::default(),
            ribbon: RibbonRepStyle::default(),
            cartoon: CartoonRepStyle::default(),
            ball_stick: BallStickRepStyle::default(),
            spheres: SphereRepStyle::default(),
        }
    }
}

impl GarnishViewSettings {
    pub fn step_animations(&mut self, dt: f32, rate: f32) -> bool {
        self.surface_alpha.step(dt, rate)
            | self.ribbon_alpha.step(dt, rate)
            | self.cartoon_alpha.step(dt, rate)
            | self.ball_stick_alpha.step(dt, rate)
            | self.spheres_alpha.step(dt, rate)
    }

    pub fn finish_animations(&mut self) {
        self.surface_alpha.finish();
        self.ribbon_alpha.finish();
        self.cartoon_alpha.finish();
        self.ball_stick_alpha.finish();
        self.spheres_alpha.finish();
    }
}
