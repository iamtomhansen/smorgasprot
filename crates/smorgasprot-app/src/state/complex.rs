//! State, layer styles, and display options for the 3D Complex workspace tab.

use super::{Choice, ExplodeParams, Tween};
use crate::camera::CameraState;
use crate::style::{AtomSizeMode, ChainColorLens, ColorLens, VisualChannel};
use egui::Color32;
use serde::{Deserialize, Serialize};
use smorgasprot_core::surface::{GeodesicPreset, SurfaceParams};

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

/// Face-culling mode for 3D molecular surface meshes.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum SurfaceCulling {
    #[default]
    None,
    Backfaces,
    Frontfaces,
}

impl Choice for SurfaceCulling {
    const ALL: &'static [Self] = &[Self::None, Self::Backfaces, Self::Frontfaces];

    fn label(self) -> &'static str {
        match self {
            Self::None => "None (Double-Sided)",
            Self::Backfaces => "Backfaces (Solid Shell)",
            Self::Frontfaces => "Frontfaces (X-Ray Cutaway)",
        }
    }
}

impl Choice for GeodesicPreset {
    const ALL: &'static [Self] = &[
        Self::Octahedron8,
        Self::Icosahedron12,
        Self::Octahedron18,
        Self::Icosahedron42,
        Self::Octahedron66,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Octahedron8 => "Octahedron 8 (2.75 Å — mobile / minimal)",
            Self::Icosahedron12 => "Icosahedron 12 (2.20 Å — coarse)",
            Self::Octahedron18 => "Octahedron 18 (1.75 Å — fast display)",
            Self::Icosahedron42 => "Icosahedron 42 (1.45 Å — medium-fine)",
            Self::Octahedron66 => "Octahedron 66 (1.20 Å — fine)",
        }
    }
}

/// Visual channels and representation settings for the 3D molecular surface mesh layer.
#[derive(Clone, Debug)]
pub struct SurfaceLayerStyle {
    pub visible: bool,
    pub alpha: Tween,
    pub culling: SurfaceCulling,
    pub color: VisualChannel<ChainColorLens, Color32>,
    /// Presentation-only tessellation preset (separate from analytical `Octahedron66`).
    pub draw_preset: GeodesicPreset,
    /// Presentation-only meshing parameters (never fed into analytical calculations).
    pub draw_mesh: SurfaceParams,
}

impl Default for SurfaceLayerStyle {
    fn default() -> Self {
        Self {
            visible: true,
            alpha: Tween::new(0.82),
            culling: SurfaceCulling::None,
            color: VisualChannel::new(ChainColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            draw_preset: GeodesicPreset::Octahedron18,
            draw_mesh: SurfaceParams::display(),
        }
    }
}

impl SurfaceLayerStyle {
    pub fn is_default_channels(&self) -> bool {
        self.color.is_default() && self.culling == SurfaceCulling::None
    }

    pub fn reset_channels(&mut self) {
        self.color.reset();
        self.culling = SurfaceCulling::None;
    }

    pub fn is_default_draw_mesh(&self) -> bool {
        self.draw_preset == GeodesicPreset::Octahedron18 && self.draw_mesh == SurfaceParams::display()
    }

    pub fn reset_draw_mesh(&mut self) {
        self.draw_preset = GeodesicPreset::Octahedron18;
        self.draw_mesh = SurfaceParams::display();
    }
}

/// Visual channels and representation settings for the 3D Cα cartoon ribbon trace layer.
#[derive(Clone, Debug)]
pub struct CartoonLayerStyle {
    pub visible: bool,
    pub alpha: Tween,
    pub color: VisualChannel<ChainColorLens, Color32>,
    pub width_scale: f32,
}

impl Default for CartoonLayerStyle {
    fn default() -> Self {
        Self {
            visible: true,
            alpha: Tween::new(0.70),
            color: VisualChannel::new(ChainColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            width_scale: 1.0,
        }
    }
}

impl CartoonLayerStyle {
    pub fn is_default_channels(&self) -> bool {
        self.color.is_default() && (self.width_scale - 1.0).abs() < 1e-4
    }

    pub fn reset_channels(&mut self) {
        self.color.reset();
        self.width_scale = 1.0;
    }
}

/// Representation settings for the derived 3D interface boundary mesh layer.
#[derive(Clone, Debug)]
pub struct BoundaryLayerStyle {
    pub mode: InterfaceDisplayMode,
    pub alpha: Tween,
    pub show_wireframe: bool,
}

impl Default for BoundaryLayerStyle {
    fn default() -> Self {
        Self {
            mode: InterfaceDisplayMode::SmoothedSdf,
            alpha: Tween::new(0.85),
            show_wireframe: true,
        }
    }
}

/// Visual channels and representation settings for the 3D interfacial heavy-atom sphere layer.
#[derive(Clone, Debug)]
pub struct SphereLayerStyle {
    pub visible: bool,
    pub alpha: Tween,
    pub fill: VisualChannel<ColorLens, Color32>,
    pub radius: VisualChannel<AtomSizeMode, f32>,
}

impl Default for SphereLayerStyle {
    fn default() -> Self {
        Self {
            visible: false,
            alpha: Tween::new(0.90),
            fill: VisualChannel::new(ColorLens::InterfaceSide, Color32::from_rgb(86, 180, 233)),
            radius: VisualChannel::new(AtomSizeMode::Fixed, 1.0),
        }
    }
}

impl SphereLayerStyle {
    pub fn is_default_channels(&self) -> bool {
        self.fill.is_default() && self.radius.is_default()
    }

    pub fn reset_channels(&mut self) {
        self.fill.reset();
        self.radius.reset();
    }
}

/// Camera, layer styles, and explode separation for the 3D Complex workspace tab.
#[derive(Clone, Debug)]
pub struct ComplexViewSettings {
    pub camera: CameraState,
    pub surface: SurfaceLayerStyle,
    pub cartoon: CartoonLayerStyle,
    pub boundary: BoundaryLayerStyle,
    pub spheres: SphereLayerStyle,
    /// Explode separation along ±n, `0` (closed) to `1` (fully apart).
    pub explode: Tween,
}

impl Default for ComplexViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            surface: SurfaceLayerStyle::default(),
            cartoon: CartoonLayerStyle::default(),
            boundary: BoundaryLayerStyle::default(),
            spheres: SphereLayerStyle::default(),
            explode: Tween::new(0.0),
        }
    }
}

impl ComplexViewSettings {
    /// Current separation between the two groups in Ångströms.
    pub fn explode_distance_a(&self, params: &ExplodeParams) -> f32 {
        self.explode.value * params.max_explode_a
    }

    /// Advances all tweens in the Complex tab; returns `true` while any tween is moving.
    pub fn step_animations(&mut self, dt: f32, rate: f32) -> bool {
        self.explode.step(dt, rate)
            | self.surface.alpha.step(dt, rate)
            | self.cartoon.alpha.step(dt, rate)
            | self.boundary.alpha.step(dt, rate)
            | self.spheres.alpha.step(dt, rate)
    }

    /// Immediately snaps all Complex tab tweens to their targets.
    pub fn finish_animations(&mut self) {
        self.explode.finish();
        self.surface.alpha.finish();
        self.cartoon.alpha.finish();
        self.boundary.alpha.finish();
        self.spheres.alpha.finish();
    }
}
