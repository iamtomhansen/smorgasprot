//! UI session state: everything the user can change that is not part of a dataset.
//! Global session state lives here; each workspace tab encapsulates its state in a submodule.

pub mod cartography;
pub mod complex;
mod tween;
pub mod view_params;

pub use crate::style::{
    AtomSizeMode, BFactorMode, ChainColorLens, ChannelPreset, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder,
    LabelValueChannel, NodeStrokeColorLens, ResidueNameStyle, ScalarMetric, TerritoryColorLens, TerritoryStrokeLens,
    VisualChannel,
};
pub use cartography::{AtomLayerStyle, CartographyViewSettings, EdgeLayerStyle, TerritoryLayerStyle};
pub use complex::{
    BoundaryLayerStyle, CartoonLayerStyle, ComplexViewSettings, InterfaceDisplayMode, SphereLayerStyle,
    SurfaceCulling, SurfaceLayerStyle,
};
pub use tween::Tween;
pub use view_params::{
    AtomDrawParams, ConfigParams, ExplodeParams, GridParams, LensParams, MorphParams, NavigationParams,
    OpacityParams, RenderBackend, RenderingSettings, StatusParams, ViewParams,
};

use crate::camera::CameraState;
use crate::style::Palette;
use serde::{Deserialize, Serialize};
use smorgasprot_core::{GlobalAtomIdx, GlobalChainIdx, GlobalResidueIdx};

/// A closed set of labelled options, rendered by `panels::widgets::choice_combo`.
pub trait Choice: Copy + PartialEq + 'static {
    /// Every option in display order.
    const ALL: &'static [Self];
    /// Human-readable label for menus.
    fn label(self) -> &'static str;
}

/// Top-level workspace tab (Blender-style).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    /// 3D macromolecular complex assembly setup: surfaces, cartoon, explode.
    Complex,
    /// Unwrapped 2D/3D interface cartography ("Sandwich" workspace).
    Cartography,
}

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
    pub cartography: CartographyViewSettings,
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
            cartography: CartographyViewSettings::default(),
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
            WorkspaceTab::Cartography => &self.cartography.camera,
        }
    }

    /// Mutable camera of the currently active workspace tab.
    pub fn active_camera_mut(&mut self) -> &mut CameraState {
        match self.active_tab {
            WorkspaceTab::Complex => &mut self.complex.camera,
            WorkspaceTab::Cartography => &mut self.cartography.camera,
        }
    }

    /// Switches to `tab`, snapping any in-flight animations on the outgoing tab so hidden tabs never tick.
    pub fn switch_tab(&mut self, tab: WorkspaceTab) {
        if self.active_tab != tab {
            match self.active_tab {
                WorkspaceTab::Complex => self.complex.finish_animations(),
                WorkspaceTab::Cartography => self.cartography.finish_animations(),
            }
            self.active_tab = tab;
        }
    }

    /// Advances animations on the active workspace tab; returns `true` if a repaint is needed.
    pub fn step_animations(&mut self, dt: f32) -> bool {
        let rate = self.view_params.navigation.animation_rate_per_s;
        match self.active_tab {
            WorkspaceTab::Complex => self.complex.step_animations(dt, rate),
            WorkspaceTab::Cartography => self.cartography.step_animations(dt, rate),
        }
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
