//! UI session state: everything the user can change that is not part of a dataset.
//! Panels edit these settings directly; app-level side effects go through [`Action`].

mod options;
mod tween;
pub mod view_params;

pub use options::{AtomSizeMode, BFactorMode, Choice, ColorLens, InterfaceDisplayMode, WorkspaceTab};
pub use tween::Tween;
pub use view_params::{LensParams, NavigationParams, RenderParams, ViewParams};

use crate::camera::CameraState;
use smorgasprot_core::{AtomNodeIdx, InterfaceSide, TerritoryIdx};

/// Full explode separation between the two groups (Å).
pub const MAX_EXPLODE_A: f32 = 24.0;
/// How long a status-bar message stays visible (s).
const STATUS_LIFETIME_S: f64 = 6.0;

/// App-level commands emitted by panels and applied once per frame by `SmorgasprotApp`.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Switch the active dataset (clears selection).
    SelectDataset(usize),
    /// Write the standalone HTML report for all datasets.
    ExportHtml,
}

/// Transient status-bar message; times come from `egui::InputState::time` (wasm-safe).
#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub expires_at: f64,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>, now: f64) -> Self {
        Self { text: text.into(), is_error: false, expires_at: now + STATUS_LIFETIME_S }
    }

    pub fn error(text: impl Into<String>, now: f64) -> Self {
        Self { text: text.into(), is_error: true, expires_at: now + STATUS_LIFETIME_S }
    }
}

/// Complete per-session UI state, owned by the app and shared by panels and viewport.
#[derive(Clone, Debug)]
pub struct UiState {
    pub tab: WorkspaceTab,
    pub camera: CameraState,
    pub complex: ComplexViewSettings,
    pub interface: InterfaceViewSettings,
    pub selection: Selection,
    pub view_params: ViewParams,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            tab: WorkspaceTab::Complex,
            camera: CameraState::default(),
            complex: ComplexViewSettings::default(),
            interface: InterfaceViewSettings::default(),
            selection: Selection::default(),
            view_params: ViewParams::default(),
        }
    }
}

impl UiState {
    /// Advances all slider animations; returns `true` if a repaint is needed.
    pub fn step_animations(&mut self, dt: f32) -> bool {
        // Non-short-circuiting `|` so every tween advances each frame.
        self.complex.explode.step(dt) | self.interface.flatten.step(dt) | self.interface.book.step(dt)
    }
}

/// Representation toggles and opacities for the Complex tab.
#[derive(Clone, Debug)]
pub struct ComplexViewSettings {
    pub show_surface: bool,
    pub surface_alpha: f32,
    pub cull_backfaces: bool,
    pub show_cartoon: bool,
    pub cartoon_alpha: f32,
    pub interface_mode: InterfaceDisplayMode,
    pub interface_alpha: f32,
    pub show_spheres: bool,
    pub spheres_alpha: f32,
    /// Explode separation along ±n, `0` (closed) to `1` (fully apart).
    pub explode: Tween,
}

impl Default for ComplexViewSettings {
    fn default() -> Self {
        Self {
            show_surface: true,
            surface_alpha: 0.82,
            cull_backfaces: false,
            show_cartoon: true,
            cartoon_alpha: 0.70,
            interface_mode: InterfaceDisplayMode::SmoothedSdf,
            interface_alpha: 0.85,
            show_spheres: false,
            spheres_alpha: 0.90,
            explode: Tween::new(0.0),
        }
    }
}

impl ComplexViewSettings {
    /// Current separation between the two groups in Ångström.
    pub fn explode_distance_a(&self) -> f32 {
        self.explode.value * MAX_EXPLODE_A
    }
}

/// Morph, lens, and layer settings for the Interface tab.
#[derive(Clone, Debug)]
pub struct InterfaceViewSettings {
    /// 3D (`0`) → bas-relief → flat 2D map (`1`).
    pub flatten: Tween,
    /// Overlay (`0`) → open-book butterfly (`1` = 180°).
    pub book: Tween,
    /// Spring-relaxed `(u, v)` instead of the raw tangent-plane projection.
    pub use_relaxed_uv: bool,
    pub lens: ColorLens,
    pub bfactor_mode: BFactorMode,
    pub size_mode: AtomSizeMode,
    pub side_a_alpha: f32,
    pub side_b_alpha: f32,
    pub show_territories: bool,
    pub show_intra_edges: bool,
    pub show_cross_arcs: bool,
    pub show_backbone: bool,
}

impl Default for InterfaceViewSettings {
    fn default() -> Self {
        Self {
            flatten: Tween::new(1.0),
            book: Tween::new(0.0),
            use_relaxed_uv: true,
            lens: ColorLens::Interactions,
            bfactor_mode: BFactorMode::Crystallographic,
            size_mode: AtomSizeMode::SqrtBuriedSasa,
            side_a_alpha: 1.0,
            side_b_alpha: 0.35,
            show_territories: true,
            show_intra_edges: true,
            show_cross_arcs: true,
            show_backbone: true,
        }
    }
}

impl InterfaceViewSettings {
    /// Layer opacity for one side of the interface (`0` for unassigned).
    pub fn side_alpha(&self, side: InterfaceSide) -> f32 {
        match side {
            InterfaceSide::SideA => self.side_a_alpha,
            InterfaceSide::SideB => self.side_b_alpha,
            InterfaceSide::None => 0.0,
        }
    }

    /// Shows one side at full opacity and nearly hides the other.
    pub fn solo(&mut self, side: InterfaceSide) {
        let (a, b) = match side {
            InterfaceSide::SideA => (1.0, 0.05),
            InterfaceSide::SideB => (0.05, 1.0),
            InterfaceSide::None => return,
        };
        self.side_a_alpha = a;
        self.side_b_alpha = b;
    }

    /// Shows both sides, keeping B slightly translucent so A reads as foreground.
    pub fn show_both(&mut self) {
        self.side_a_alpha = 1.0;
        self.side_b_alpha = 0.85;
    }
}

/// Hover and click selection in the Interface tab (indices into the active dataset's map).
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub hovered_atom: Option<AtomNodeIdx>,
    pub selected_territory: Option<TerritoryIdx>,
}

impl Selection {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
