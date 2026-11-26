//! State, interface layers, and explode separation for the 3D Complex workspace tab.

use super::garnish::SphereRepStyle;
use super::presentation::{MeshShading, PresentationStyle, SurfaceCulling, WireframeStyle};
use super::{ExplodeParams, Tween};
use crate::camera::CameraState;
use serde::{Deserialize, Serialize};

/// Which derived interface geometry layer is drawn in the Slice tab.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum InterfaceDisplayMode {
    SmoothedSdf,
    ContactEnvelope,
    NegativeSpace,
    CavitiesBuried,
    CavitiesChannel,
}

/// Presentation settings for a derived 3D interface boundary mesh layer.
#[derive(Clone, Debug)]
pub struct BoundaryLayerStyle {
    pub visible: bool,
    pub alpha: Tween,
    pub presentation: PresentationStyle<()>,
}

impl Default for BoundaryLayerStyle {
    fn default() -> Self {
        Self::new(false)
    }
}

impl BoundaryLayerStyle {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            alpha: Tween::new(0.85),
            presentation: Self::default_presentation(),
        }
    }

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
    pub show_ribbon: bool,
    pub ribbon_alpha: Tween,
    pub show_cartoon: bool,
    pub cartoon_alpha: Tween,
    pub show_ball_stick: bool,
    pub ball_stick_alpha: Tween,
    pub show_spheres: bool,
    pub spheres_alpha: Tween,
    pub interface_sheet: BoundaryLayerStyle,
    pub contact_envelope: BoundaryLayerStyle,
    pub negative_space: BoundaryLayerStyle,
    pub cavities: BoundaryLayerStyle,
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
            show_ribbon: true,
            ribbon_alpha: Tween::new(0.70),
            show_cartoon: false,
            cartoon_alpha: Tween::new(0.92),
            show_ball_stick: false,
            ball_stick_alpha: Tween::new(1.0),
            show_spheres: false,
            spheres_alpha: Tween::new(0.90),
            interface_sheet: BoundaryLayerStyle::new(true),
            contact_envelope: BoundaryLayerStyle::new(false),
            negative_space: BoundaryLayerStyle::new(false),
            cavities: BoundaryLayerStyle::new(false),
            interface_spheres_visible: false,
            interface_spheres_alpha: Tween::new(0.90),
            interface_spheres: SphereRepStyle::default(),
            explode: Tween::new(0.0),
            complex_explode: Tween::new(0.0),
        }
    }
}

impl SliceViewSettings {
    /// All derived 3D interface boundary layers paired with their geometry mode.
    pub fn boundary_layers(&self) -> [(InterfaceDisplayMode, &BoundaryLayerStyle); 5] {
        [
            (InterfaceDisplayMode::SmoothedSdf, &self.interface_sheet),
            (InterfaceDisplayMode::ContactEnvelope, &self.contact_envelope),
            (InterfaceDisplayMode::NegativeSpace, &self.negative_space),
            (InterfaceDisplayMode::CavitiesBuried, &self.cavities),
            (InterfaceDisplayMode::CavitiesChannel, &self.cavities),
        ]
    }

    /// Current separation between the two groups in Ångströms.
    pub fn explode_distance_a(&self, params: &ExplodeParams) -> f32 {
        self.explode.value * params.max_explode_a
    }

    /// Advances all tweens in the Slice tab; returns `true` while any tween is moving.
    pub fn step_animations(&mut self, dt: f32, rate: f32) -> bool {
        self.explode.step(dt, rate)
            | self.complex_explode.step(dt, rate)
            | self.surface_alpha.step(dt, rate)
            | self.ribbon_alpha.step(dt, rate)
            | self.cartoon_alpha.step(dt, rate)
            | self.ball_stick_alpha.step(dt, rate)
            | self.spheres_alpha.step(dt, rate)
            | self.interface_sheet.alpha.step(dt, rate)
            | self.contact_envelope.alpha.step(dt, rate)
            | self.negative_space.alpha.step(dt, rate)
            | self.cavities.alpha.step(dt, rate)
            | self.interface_spheres_alpha.step(dt, rate)
    }

    /// Immediately snaps all Slice tab tweens to their targets.
    pub fn finish_animations(&mut self) {
        self.explode.finish();
        self.complex_explode.finish();
        self.surface_alpha.finish();
        self.ribbon_alpha.finish();
        self.cartoon_alpha.finish();
        self.ball_stick_alpha.finish();
        self.spheres_alpha.finish();
        self.interface_sheet.alpha.finish();
        self.contact_envelope.alpha.finish();
        self.negative_space.alpha.finish();
        self.cavities.alpha.finish();
        self.interface_spheres_alpha.finish();
    }
}
