//! UI session state: everything the user can change that is not part of a dataset.
//! Each workspace tab owns its camera and display settings; selection, panel layout, and view parameters are shared globally.

mod options;
mod tween;
pub mod view_params;

pub use options::{AtomSizeMode, BFactorMode, Choice, ColorLens, InterfaceDisplayMode, WorkspaceTab};
pub use tween::Tween;
pub use view_params::{
    AtomDrawParams, ExplodeParams, GridParams, LensParams, MorphParams, NavigationParams, OpacityParams, StatusParams,
    ViewParams,
};

use crate::camera::CameraState;
use crate::style::Palette;
use smorgasprot_core::{GlobalAtomIdx, GlobalChainIdx, GlobalResidueIdx, InterfaceSide};

/// Visibility toggles for collapsible side/bottom panels and floating windows.
#[derive(Clone, Debug)]
pub struct PanelVisibility {
    pub left_controls: bool,
    pub right_inspector: bool,
    pub status_bar: bool,
    pub preferences: bool,
}

impl Default for PanelVisibility {
    fn default() -> Self {
        Self {
            left_controls: true,
            right_inspector: true,
            status_bar: true,
            preferences: false,
        }
    }
}

/// Complete per-session UI state, owned by the app and shared by panels and viewport.
#[derive(Clone, Debug)]
pub struct UiState {
    pub active_tab: WorkspaceTab,
    pub panels: PanelVisibility,
    pub complex: ComplexViewSettings,
    pub interface: InterfaceViewSettings,
    pub selection: Selection,
    pub view_params: ViewParams,
    pub palette: Palette,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_tab: WorkspaceTab::Complex,
            panels: PanelVisibility::default(),
            complex: ComplexViewSettings::default(),
            interface: InterfaceViewSettings::default(),
            selection: Selection::default(),
            view_params: ViewParams::default(),
            palette: Palette::load_default(),
        }
    }
}

impl UiState {
    /// Camera of the currently active workspace tab.
    pub fn active_camera(&self) -> &CameraState {
        match self.active_tab {
            WorkspaceTab::Complex => &self.complex.camera,
            WorkspaceTab::Interface => &self.interface.camera,
        }
    }

    /// Mutable camera of the currently active workspace tab.
    pub fn active_camera_mut(&mut self) -> &mut CameraState {
        match self.active_tab {
            WorkspaceTab::Complex => &mut self.complex.camera,
            WorkspaceTab::Interface => &mut self.interface.camera,
        }
    }

    /// Switches to `tab`, snapping any in-flight animations on the outgoing tab so hidden tabs never tick.
    pub fn switch_tab(&mut self, tab: WorkspaceTab) {
        if self.active_tab != tab {
            match self.active_tab {
                WorkspaceTab::Complex => self.complex.finish_animations(),
                WorkspaceTab::Interface => self.interface.finish_animations(),
            }
            self.active_tab = tab;
        }
    }

    /// Advances animations on the active workspace tab; returns `true` if a repaint is needed.
    pub fn step_animations(&mut self, dt: f32) -> bool {
        let rate = self.view_params.navigation.animation_rate_per_s;
        match self.active_tab {
            WorkspaceTab::Complex => self.complex.step_animations(dt, rate),
            WorkspaceTab::Interface => self.interface.step_animations(dt, rate),
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

/// Camera, morph, lens, and tweened layer settings for the Interface workspace tab.
#[derive(Clone, Debug)]
pub struct InterfaceViewSettings {
    pub camera: CameraState,
    /// 3D (`0`) → bas-relief → flat 2D map (`1`).
    pub flatten: Tween,
    /// Overlay (`0`) → open-book butterfly (`1` = 180°).
    pub book: Tween,
    /// Spring-relaxed `(u, v)` instead of the raw tangent-plane projection.
    pub use_relaxed_uv: bool,
    pub lens: ColorLens,
    pub bfactor_mode: BFactorMode,
    pub size_mode: AtomSizeMode,
    pub side_a_alpha: Tween,
    pub side_b_alpha: Tween,
    pub show_territories: bool,
    pub show_intra_edges: bool,
    pub show_cross_arcs: bool,
    pub show_backbone: bool,
}

impl Default for InterfaceViewSettings {
    fn default() -> Self {
        Self {
            camera: CameraState::default(),
            flatten: Tween::new(1.0),
            book: Tween::new(0.0),
            use_relaxed_uv: true,
            lens: ColorLens::Interactions,
            bfactor_mode: BFactorMode::Crystallographic,
            size_mode: AtomSizeMode::SqrtBuriedSasa,
            side_a_alpha: Tween::new(1.0),
            side_b_alpha: Tween::new(0.35),
            show_territories: true,
            show_intra_edges: true,
            show_cross_arcs: true,
            show_backbone: true,
        }
    }
}

impl InterfaceViewSettings {
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

/// Hover and active selection shared across all workspace tabs (canonical complex indices).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub hovered_atom: Option<GlobalAtomIdx>,
    pub selected_atom: Option<GlobalAtomIdx>,
    pub selected_residue: Option<GlobalResidueIdx>,
    pub selected_chain: Option<GlobalChainIdx>,
}

impl Selection {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
