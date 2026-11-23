//! UI session state: everything the user can change that is not part of a dataset.
//! Each workspace tab owns its camera and display settings; selection, panel layout, and view parameters are shared globally.

mod options;
mod tween;
pub mod view_params;

pub use options::{
    AtomSizeMode, BFactorMode, Choice, ColorLens, EdgeColorLens, EdgeWidthLens, InterfaceDisplayMode, LabelBuilder,
    LabelValueChannel, NodeStrokeColorLens, ResidueNameStyle, TerritoryColorLens, TerritoryStrokeLens, WorkspaceTab,
};
pub use tween::Tween;
pub use view_params::{
    AtomDrawParams, ExplodeParams, GridParams, LensParams, MorphParams, NavigationParams, OpacityParams, StatusParams,
    ViewParams,
};

use crate::camera::CameraState;
use crate::style::{ChromePalette, Palette};
use egui::Color32;
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

/// Camera, morph, visual-channel lenses, and tweened layer settings for the Interface workspace tab.
#[derive(Clone, Debug)]
pub struct InterfaceViewSettings {
    pub camera: CameraState,
    /// 3D (`0`) → bas-relief → flat 2D map (`1`).
    pub flatten: Tween,
    /// Overlay (`0`) → open-book butterfly (`1` = 180°).
    pub book: Tween,
    /// Spring-relaxed `(u, v)` instead of the raw tangent-plane projection.
    pub use_relaxed_uv: bool,

    // Visual channel lenses
    pub lens: ColorLens,
    pub size_mode: AtomSizeMode,
    pub stroke_color_lens: NodeStrokeColorLens,
    pub stroke_width_lens: AtomSizeMode,
    pub edge_color_lens: EdgeColorLens,
    pub edge_width_lens: EdgeWidthLens,
    pub territory_color_lens: TerritoryColorLens,
    pub territory_stroke_lens: TerritoryStrokeLens,

    // Per-channel customization toggles (unchecked falls back to default channel behavior)
    pub lens_enabled: bool,
    pub size_mode_enabled: bool,
    pub stroke_color_enabled: bool,
    pub stroke_width_enabled: bool,
    pub edge_color_enabled: bool,
    pub edge_width_enabled: bool,
    pub territory_color_enabled: bool,
    pub territory_stroke_enabled: bool,

    // User-configurable constant values when a channel is set to `Fixed`
    pub fixed_node_fill: Color32,
    pub fixed_node_radius_scale: f32,
    pub fixed_node_stroke_color: Color32,
    pub fixed_node_stroke_width_px: f32,
    pub fixed_edge_color: Color32,
    pub fixed_edge_width_px: f32,
    pub fixed_territory_fill: Color32,
    pub fixed_territory_stroke_width_px: f32,

    // Composable label builders for atom nodes and residue territories
    pub atom_labels: LabelBuilder,
    pub territory_labels: LabelBuilder,

    pub bfactor_mode: BFactorMode,
    pub side_a_alpha: Tween,
    pub side_b_alpha: Tween,
    pub show_territories: bool,
    pub show_intra_edges: bool,
    pub show_cross_arcs: bool,
    pub show_backbone: bool,
}

impl Default for InterfaceViewSettings {
    fn default() -> Self {
        let chrome = ChromePalette::default();
        Self {
            camera: CameraState::default(),
            flatten: Tween::new(1.0),
            book: Tween::new(0.0),
            use_relaxed_uv: true,
            lens: ColorLens::Interactions,
            size_mode: AtomSizeMode::SqrtBuriedSasa,
            stroke_color_lens: NodeStrokeColorLens::DefaultInk,
            stroke_width_lens: AtomSizeMode::Fixed,
            edge_color_lens: EdgeColorLens::InteractionKind,
            edge_width_lens: EdgeWidthLens::InteractionKind,
            territory_color_lens: TerritoryColorLens::InterfaceSide,
            territory_stroke_lens: TerritoryStrokeLens::Fixed,
            lens_enabled: true,
            size_mode_enabled: true,
            stroke_color_enabled: false,
            stroke_width_enabled: false,
            edge_color_enabled: true,
            edge_width_enabled: true,
            territory_color_enabled: true,
            territory_stroke_enabled: false,
            fixed_node_fill: Color32::from_rgb(86, 180, 233),
            fixed_node_radius_scale: 1.15,
            fixed_node_stroke_color: chrome.ink,
            fixed_node_stroke_width_px: 1.0,
            fixed_edge_color: Color32::from_rgb(135, 206, 250),
            fixed_edge_width_px: 1.5,
            fixed_territory_fill: Color32::from_rgb(86, 180, 233),
            fixed_territory_stroke_width_px: 1.2,
            atom_labels: LabelBuilder::default_atom(),
            territory_labels: LabelBuilder::default_territory(),
            bfactor_mode: BFactorMode::Crystallographic,
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
    /// Active node fill lens (falls back to `ColorLens::default()` when channel customization is unchecked).
    pub fn effective_lens(&self) -> ColorLens {
        if self.lens_enabled { self.lens } else { ColorLens::default() }
    }

    /// Active node size lens (falls back to `AtomSizeMode::default()` when unchecked).
    pub fn effective_size_mode(&self) -> AtomSizeMode {
        if self.size_mode_enabled { self.size_mode } else { AtomSizeMode::default() }
    }

    /// Active node border color lens (falls back to `NodeStrokeColorLens::default()` when unchecked).
    pub fn effective_stroke_color_lens(&self) -> NodeStrokeColorLens {
        if self.stroke_color_enabled { self.stroke_color_lens } else { NodeStrokeColorLens::default() }
    }

    /// Active node border width lens (falls back to `AtomSizeMode::Fixed` at `1.0 px` when unchecked).
    pub fn effective_stroke_width_lens(&self) -> (AtomSizeMode, f32) {
        if self.stroke_width_enabled {
            (self.stroke_width_lens, self.fixed_node_stroke_width_px)
        } else {
            (AtomSizeMode::Fixed, 1.0)
        }
    }

    /// Active contact edge color lens (falls back to `EdgeColorLens::default()` when unchecked).
    pub fn effective_edge_color_lens(&self) -> EdgeColorLens {
        if self.edge_color_enabled { self.edge_color_lens } else { EdgeColorLens::default() }
    }

    /// Active contact edge width lens (falls back to `EdgeWidthLens::default()` when unchecked).
    pub fn effective_edge_width_lens(&self) -> EdgeWidthLens {
        if self.edge_width_enabled { self.edge_width_lens } else { EdgeWidthLens::default() }
    }

    /// Active territory fill color lens (falls back to `TerritoryColorLens::default()` when unchecked).
    pub fn effective_territory_color_lens(&self) -> TerritoryColorLens {
        if self.territory_color_enabled { self.territory_color_lens } else { TerritoryColorLens::default() }
    }

    /// Active territory border stroke width lens (falls back to `TerritoryStrokeLens::Fixed` at `1.2 px` when unchecked).
    pub fn effective_territory_stroke_lens(&self) -> (TerritoryStrokeLens, f32) {
        if self.territory_stroke_enabled {
            (self.territory_stroke_lens, self.fixed_territory_stroke_width_px)
        } else {
            (TerritoryStrokeLens::Fixed, 1.2)
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
