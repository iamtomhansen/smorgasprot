//! PyMOL/ChimeraX-style Garnish presentation settings, carried over into Complex mode.

use super::presentation::{MeshShading, PresentationStyle, SphereQuality, SurfaceCulling, CartoonStyle};
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

/// Cα cartoon ribbon representation style & channels.
#[derive(Clone, Debug)]
pub struct CartoonRepStyle {
    pub color: VisualChannel<ChainColorLens, Color32>,
    pub width_scale: f32,
    pub presentation: PresentationStyle<CartoonStyle>,
}

impl Default for CartoonRepStyle {
    fn default() -> Self {
        Self {
            color: VisualChannel::new(ChainColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            width_scale: 1.0,
            presentation: Self::default_presentation(),
        }
    }
}

impl CartoonRepStyle {
    pub fn default_presentation() -> PresentationStyle<CartoonStyle> {
        PresentationStyle {
            quality: CartoonStyle::Capsule2D,
            shading: MeshShading::Smooth,
            culling: SurfaceCulling::Backfaces,
            ..PresentationStyle::default()
        }
    }

    pub fn is_default_channels(&self) -> bool {
        self.color.is_default() && (self.width_scale - 1.0).abs() < 1e-4
    }

    pub fn reset_channels(&mut self) {
        self.color.reset();
        self.width_scale = 1.0;
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

/// State for the Garnish ("3. Garnish") presentation calibration workspace.
#[derive(Clone, Debug)]
pub struct GarnishViewSettings {
    pub camera: CameraState,
    pub surface_visible: bool,
    pub surface_alpha: Tween,
    pub cartoon_visible: bool,
    pub cartoon_alpha: Tween,
    pub spheres_visible: bool,
    pub spheres_alpha: Tween,
    pub bfactor_mode: BFactorMode,
    pub surface: SurfaceRepStyle,
    pub cartoon: CartoonRepStyle,
    pub spheres: SphereRepStyle,
}

impl Default for GarnishViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            surface_visible: true,
            surface_alpha: Tween::new(0.82),
            cartoon_visible: true,
            cartoon_alpha: Tween::new(0.70),
            spheres_visible: false,
            spheres_alpha: Tween::new(0.90),
            bfactor_mode: BFactorMode::default(),
            surface: SurfaceRepStyle::default(),
            cartoon: CartoonRepStyle::default(),
            spheres: SphereRepStyle::default(),
        }
    }
}

impl GarnishViewSettings {
    pub fn step_animations(&mut self, dt: f32, rate: f32) -> bool {
        self.surface_alpha.step(dt, rate)
            | self.cartoon_alpha.step(dt, rate)
            | self.spheres_alpha.step(dt, rate)
    }

    pub fn finish_animations(&mut self) {
        self.surface_alpha.finish();
        self.cartoon_alpha.finish();
        self.spheres_alpha.finish();
    }
}
