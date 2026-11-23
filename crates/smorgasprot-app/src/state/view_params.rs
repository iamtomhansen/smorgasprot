//! Tunable display and navigation parameter groups (app-side counterpart of core `AnalysisParams`).
//! Feature-scoped groups keep domain tunables separate from unrelated rendering settings.

use serde::{Deserialize, Serialize};
use smorgasprot_core::param_group;
use smorgasprot_core::params::ParamGroup;

/// All app-side parameter groups; saved in preferences and sessions.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ViewParams {
    pub explode: ExplodeParams,
    pub morph: MorphParams,
    pub grid: GridParams,
    pub opacity: OpacityParams,
    pub atom_draw: AtomDrawParams,
    pub lenses: LensParams,
    pub navigation: NavigationParams,
    pub status: StatusParams,
}

impl ViewParams {
    /// All groups in display order, for generic editors.
    pub fn groups_mut(&mut self) -> [&mut dyn ParamGroup; 8] {
        [
            &mut self.explode,
            &mut self.morph,
            &mut self.grid,
            &mut self.opacity,
            &mut self.atom_draw,
            &mut self.lenses,
            &mut self.navigation,
            &mut self.status,
        ]
    }
}

param_group! {
    /// Controls for 3D complex sandwich separation along the interface normal.
    pub struct ExplodeParams("explode", "Complex Explode") {
        /// Separation between the two groups at full explode (Å).
        max_explode_a: f32 = 24.0, 0.0..=100.0, "Max explode distance", "Å";
    }
}

param_group! {
    /// Geometry parameters for 3D-to-2D flattening and open-book butterfly hinging.
    pub struct MorphParams("morph", "Interface Morph & Butterfly") {
        /// Offset along ±n keeping the two flat sheets from z-fighting (Å).
        sheet_offset_a: f32 = 0.08, 0.0..=2.0, "Dual-sheet z-offset", "Å";
        /// Half-length of the drawn open-book hinge line (Å).
        hinge_half_len_a: f32 = 28.0, 0.0..=100.0, "Hinge line half-length", "Å";
    }
}

param_group! {
    /// Dimensions of the tangent-plane reference grid.
    pub struct GridParams("grid", "Reference Grid") {
        /// Half-width of the tangent-plane reference grid (Å).
        grid_extent_a: f32 = 40.0, 0.0..=200.0, "Reference grid extent", "Å";
        /// Reference grid spacing (Å).
        grid_step_a: f32 = 5.0, 1.0..=50.0, "Reference grid spacing", "Å";
    }
}

param_group! {
    /// Opacity cutoffs and dimming scales for scene layers.
    pub struct OpacityParams("opacity", "Layer Visibility & Opacity") {
        /// Layers at or below this opacity are not drawn.
        min_draw_alpha: f32 = 0.02, 0.0..=0.5, "Draw opacity cutoff", "";
        /// Sides at or below this opacity are ghosted: no picking, springs, or arcs.
        min_interactive_alpha: f32 = 0.05, 0.0..=0.5, "Interactive opacity cutoff", "";
        /// Opacity multiplier for unassigned chains in the Complex tab.
        unassigned_alpha_scale: f32 = 0.45, 0.0..=1.0, "Unassigned chain opacity scale", "";
    }
}

param_group! {
    /// Screen-space sizing of atom discs and backbone traces.
    pub struct AtomDrawParams("atom_draw", "Atom & Ribbon Sizing") {
        /// Smallest drawn atom radius (px).
        min_atom_radius_px: f32 = 3.5, 0.5..=20.0, "Min atom radius", "px";
        /// Atom base radius as a fraction of zoom (px per Å).
        atom_radius_per_zoom: f32 = 0.28, 0.05..=1.0, "Atom radius zoom scale", "";
    }
}

param_group! {
    /// Thresholds and continuous normalization ranges for visual-channel lenses.
    pub struct LensParams("lenses", "Color & Sizing Lens Ranges") {
        /// Aliphatic score at or above which the Chemistry lens colors an atom aliphatic.
        aliphatic_threshold: f32 = 0.75, 0.0..=1.0, "Aliphatic cutoff", "";
        /// Overlap above which the Interactions lens colors an atom as clashing (Å).
        clash_threshold_a: f32 = 0.05, 0.0..=1.0, "Clash overlap threshold", "Å";
        /// B-factor upper bound of the "rigid" bin (Å²).
        bfactor_rigid_max_a2: f32 = 30.0, 0.0..=200.0, "Rigid B-factor max", "Å²";
        /// B-factor upper bound of the "ordered" bin (Å²).
        bfactor_ordered_max_a2: f32 = 50.0, 0.0..=200.0, "Ordered B-factor max", "Å²";
        /// B-factor upper bound of the "flexible" bin (Å²); above is "labile".
        bfactor_flexible_max_a2: f32 = 70.0, 0.0..=200.0, "Flexible B-factor max", "Å²";
        /// pLDDT lower bound of the "very high" bin.
        plddt_very_high_min: f32 = 90.0, 0.0..=100.0, "Very high pLDDT min", "";
        /// pLDDT lower bound of the "confident" bin.
        plddt_confident_min: f32 = 70.0, 0.0..=100.0, "Confident pLDDT min", "";
        /// pLDDT lower bound of the "low" bin; below is "very low".
        plddt_low_min: f32 = 50.0, 0.0..=100.0, "Low pLDDT min", "";
        /// Atom buried ΔSASA that saturates the continuous atom color/size range (Å²).
        atom_buried_sasa_max_a2: f32 = 25.0, 1.0..=100.0, "Atom buried ΔSASA saturation", "Å²";
        /// Residue buried ΔSASA that saturates the continuous territory color/border range (Å²).
        territory_buried_sasa_max_a2: f32 = 90.0, 10.0..=300.0, "Territory buried ΔSASA saturation", "Å²";
        /// Steric clash overlap that saturates the continuous clash severity colormap/width (Å).
        clash_severity_max_a: f32 = 0.60, 0.1..=2.0, "Severe clash saturation", "Å";
        /// Signed 2D metric strain magnitude that saturates the edge strain colormap/width (Å).
        edge_strain_max_a: f32 = 1.20, 0.1..=5.0, "2D edge strain saturation", "Å";
        /// Minimum 3D contact distance for edge distance/proximity scaling (Å).
        edge_dist_min_a: f32 = 2.30, 1.0..=4.0, "Edge contact min distance", "Å";
        /// Maximum 3D contact distance for edge distance/proximity scaling (Å).
        edge_dist_max_a: f32 = 4.50, 2.5..=8.0, "Edge contact max distance", "Å";
    }
}

param_group! {
    /// Camera and pointer behavior.
    pub struct NavigationParams("navigation", "Navigation") {
        /// Orbit speed (radians per dragged pixel).
        orbit_rad_per_px: f32 = 0.008, 0.001..=0.05, "Orbit speed", "rad/px";
        /// Zoom speed per scrolled point (exponential).
        zoom_per_scroll: f32 = 0.002, 0.0002..=0.02, "Scroll zoom speed", "";
        /// Zoom after a camera reset (px per Å).
        default_zoom: f32 = 14.5, 1.0..=100.0, "Default zoom", "px/Å";
        /// Smallest allowed zoom (px per Å).
        min_zoom: f32 = 4.0, 0.5..=50.0, "Min zoom", "px/Å";
        /// Largest allowed zoom (px per Å).
        max_zoom: f32 = 75.0, 10.0..=500.0, "Max zoom", "px/Å";
        /// Pointer distance for hovering an atom (px).
        pick_radius_px: f32 = 14.0, 2.0..=50.0, "Hover pick radius", "px";
        /// Flatten amount above which left-drag pans instead of orbiting.
        flat_pan_threshold: f32 = 0.92, 0.0..=1.0, "2D pan mode threshold", "";
        /// Slider animation easing rate (1/s); higher is snappier.
        animation_rate_per_s: f32 = 12.0, 1.0..=60.0, "Animation easing rate", "1/s";
    }
}

param_group! {
    /// Status bar notification display duration (`0` = persist until dismissed).
    pub struct StatusParams("status", "Status Bar Notifications") {
        /// How long status messages stay visible before auto-hiding (0 = until dismissed).
        lifetime_s: f32 = 6.0, 0.0..=60.0, "Message lifetime (0 = until dismissed)", "s";
    }
}
