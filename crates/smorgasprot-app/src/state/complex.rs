//! State and display options for the 3D Complex workspace tab.

use super::{Choice, ExplodeParams, Tween};
use crate::camera::CameraState;
use serde::{Deserialize, Serialize};

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

/// Camera, representation toggles, and tweened opacities for the Complex workspace tab.
#[derive(Clone, Debug)]
pub struct ComplexViewSettings {
    pub camera: CameraState,
    pub show_surface: bool,
    pub surface_alpha: Tween,
    pub cull_backfaces: bool,
    pub show_cartoon: bool,
    pub cartoon_alpha: Tween,
    pub interface_mode: InterfaceDisplayMode,
    pub interface_alpha: Tween,
    pub show_spheres: bool,
    pub spheres_alpha: Tween,
    /// Explode separation along ±n, `0` (closed) to `1` (fully apart).
    pub explode: Tween,
}

impl Default for ComplexViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            show_surface: true,
            surface_alpha: Tween::new(0.82),
            cull_backfaces: false,
            show_cartoon: true,
            cartoon_alpha: Tween::new(0.70),
            interface_mode: InterfaceDisplayMode::SmoothedSdf,
            interface_alpha: Tween::new(0.85),
            show_spheres: false,
            spheres_alpha: Tween::new(0.90),
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
            | self.surface_alpha.step(dt, rate)
            | self.cartoon_alpha.step(dt, rate)
            | self.interface_alpha.step(dt, rate)
            | self.spheres_alpha.step(dt, rate)
    }

    /// Immediately snaps all Complex tab tweens to their targets.
    pub fn finish_animations(&mut self) {
        self.explode.finish();
        self.surface_alpha.finish();
        self.cartoon_alpha.finish();
        self.interface_alpha.finish();
        self.spheres_alpha.finish();
    }
}
