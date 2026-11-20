//! Application state, visual lens configuration (`RenderConfig`), and selection/edit history.

use serde::{Deserialize, Serialize};
use smorgasprot_core::{ComplexStructure, InterfaceMap, AtomNodeIdx, TerritoryIdx};

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ColorLens {
    Interactions,
    Chemistry,
    ModernCpk,
    PlddtOrBfactor,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum AtomSizeMode {
    SqrtBuriedSasa,
    Uniform,
    ClashSeverity,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RenderConfig {
    /// Continuous 3D (0.0) <-> Bas-Relief (0.5) <-> 2D Flattened (1.0) slider.
    pub flatten_t: f32,
    /// Continuous Overlay (0.0) <-> Open-Book Butterfly (1.0) hinge angle slider.
    pub book_theta: f32,
    pub color_lens: ColorLens,
    pub size_mode: AtomSizeMode,
    pub show_binder: bool,
    pub show_target: bool,
    pub binder_alpha: f32,
    pub target_alpha: f32,
    pub show_cross_contact_arcs: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            flatten_t: 0.0,
            book_theta: 0.0,
            color_lens: ColorLens::Interactions,
            size_mode: AtomSizeMode::SqrtBuriedSasa,
            show_binder: true,
            show_target: true,
            binder_alpha: 1.0,
            target_alpha: 0.35,
            show_cross_contact_arcs: true,
        }
    }
}

#[derive(Default, Debug)]
pub struct AppState {
    pub complex: Option<ComplexStructure>,
    pub interface_map: Option<InterfaceMap>,
    pub render_config: RenderConfig,
    pub selected_territory: Option<TerritoryIdx>,
    pub hovered_atom: Option<AtomNodeIdx>,
}
