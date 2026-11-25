//! State, interface layers, and explode separation for the 3D Complex workspace tab.

use super::garnish::SphereRepStyle;
use super::presentation::{MeshShading, PresentationStyle, SurfaceCulling, WireframeStyle};
use super::{ExplodeParams, Tween};
use crate::camera::CameraState;
use serde::{Deserialize, Serialize};

/// Which derived interface geometry is drawn in the Complex tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum InterfaceDisplayMode {
    SmoothedSdf,
    ContactEnvelope,
    NegativeSpace,
    Off,
}

impl super::Choice for InterfaceDisplayMode {
    const ALL: &'static [Self] = &[
        Self::SmoothedSdf,
        Self::ContactEnvelope,
        Self::NegativeSpace,
        Self::Off,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::SmoothedSdf => "Smoothed Interface Sheet",
            Self::ContactEnvelope => "Midpoint Contact Envelope",
            Self::NegativeSpace => "Negative Space (Sub-Solvent Voids)",
            Self::Off => "Off",
        }
    }
}

/// Presentation settings for the derived 3D interface boundary mesh layer.
#[derive(Clone, Debug)]
pub struct BoundaryLayerStyle {
    pub mode: InterfaceDisplayMode,
    pub alpha: Tween,
    pub presentation: PresentationStyle<()>,
}

impl Default for BoundaryLayerStyle {
    fn default() -> Self {
        Self {
            mode: InterfaceDisplayMode::SmoothedSdf,
            alpha: Tween::new(0.85),
            presentation: Self::default_presentation(),
        }
    }
}

impl BoundaryLayerStyle {
    pub fn default_presentation() -> PresentationStyle<()> {
        PresentationStyle {
            shading: MeshShading::Unlit,
            culling: SurfaceCulling::None,
            wireframe: WireframeStyle { enabled: true, width_px: 1.2 },
            ..PresentationStyle::default()
        }
    }
}

/// Camera, rudimentary Complex layer toggles, Interface Analysis layers, and explode separation.
#[derive(Clone, Debug)]
pub struct SliceViewSettings {
    pub camera: CameraState,
    pub show_surface: bool,
    pub surface_alpha: Tween,
    pub show_cartoon: bool,
    pub cartoon_alpha: Tween,
    pub show_spheres: bool,
    pub spheres_alpha: Tween,
    pub boundary: BoundaryLayerStyle,
    pub interface_spheres_visible: bool,
    pub interface_spheres_alpha: Tween,
    pub interface_spheres: SphereRepStyle,
    /// Interface explode separation along ±n, `0` (closed) to `1` (fully apart).
    pub explode: Tween,
    /// Complex-wide subunit explode separation, `0` (closed) to `1` (fully spread).
    pub complex_explode: Tween,
}

impl Default for SliceViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            show_surface: true,
            surface_alpha: Tween::new(0.82),
            show_cartoon: true,
            cartoon_alpha: Tween::new(0.70),
            show_spheres: false,
            spheres_alpha: Tween::new(0.90),
            boundary: BoundaryLayerStyle::default(),
            interface_spheres_visible: false,
            interface_spheres_alpha: Tween::new(0.90),
            interface_spheres: SphereRepStyle::default(),
            explode: Tween::new(0.0),
            complex_explode: Tween::new(0.0),
        }
    }
}

impl SliceViewSettings {
    /// Current separation between the two groups in Ångströms.
    pub fn explode_distance_a(&self, params: &ExplodeParams) -> f32 {
        self.explode.value * params.max_explode_a
    }

    /// Advances all tweens in the Complex tab; returns `true` while any tween is moving.
    pub fn step_animations(&mut self, dt: f32, rate: f32) -> bool {
        self.explode.step(dt, rate)
            | self.complex_explode.step(dt, rate)
            | self.surface_alpha.step(dt, rate)
            | self.cartoon_alpha.step(dt, rate)
            | self.spheres_alpha.step(dt, rate)
            | self.boundary.alpha.step(dt, rate)
            | self.interface_spheres_alpha.step(dt, rate)
    }

    /// Immediately snaps all Complex tab tweens to their targets.
    pub fn finish_animations(&mut self) {
        self.explode.finish();
        self.complex_explode.finish();
        self.surface_alpha.finish();
        self.cartoon_alpha.finish();
        self.spheres_alpha.finish();
        self.boundary.alpha.finish();
        self.interface_spheres_alpha.finish();
    }
}
