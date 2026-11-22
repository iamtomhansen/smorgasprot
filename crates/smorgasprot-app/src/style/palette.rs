//! Modular discrete palettes and continuous [`ColorMap`] gradients driven by `.ron` [`ColorSwatches`] files.
//! Scans `assets/palettes/*.ron` to build a dynamic palette index; only the fallback default is compiled in.

use crate::camera::ViewAxis;
use egui::Color32;
use serde::{Deserialize, Serialize};
use smorgasprot_core::{EdgeKind, Element, InterfaceSide};
use std::path::{Path, PathBuf};

/// Default directory scanned for `.ron` palette swatch files.
pub const PALETTES_DIR: &str = "assets/palettes";

/// Centralized color swatch table defining a named color scheme, loaded from or saved to `.ron` files.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ColorSwatches {
    pub name: String,
    pub primary: Color32,
    pub secondary: Color32,
    pub blue: Color32,
    pub cyan: Color32,
    pub green: Color32,
    pub yellow: Color32,
    pub orange: Color32,
    pub red: Color32,
    pub purple: Color32,
    pub neutral: Color32,
    pub carbon: Color32,
    pub mobility_stops: Vec<Color32>,
}

impl Default for ColorSwatches {
    fn default() -> Self {
        Self {
            name: "Okabe-Ito (Colorblind-Safe)".into(),
            primary: Color32::from_rgb(14, 165, 233),
            secondary: Color32::from_rgb(249, 115, 22),
            blue: Color32::from_rgb(37, 99, 235),
            cyan: Color32::from_rgb(6, 182, 212),
            green: Color32::from_rgb(34, 197, 94),
            yellow: Color32::from_rgb(250, 204, 21),
            orange: Color32::from_rgb(245, 158, 11),
            red: Color32::from_rgb(239, 68, 68),
            purple: Color32::from_rgb(168, 85, 247),
            neutral: Color32::from_rgb(148, 163, 184),
            carbon: Color32::from_rgb(71, 85, 105),
            mobility_stops: vec![
                Color32::from_rgb(0, 83, 214),
                Color32::from_rgb(101, 203, 243),
                Color32::from_rgb(255, 219, 19),
                Color32::from_rgb(255, 125, 69),
            ],
        }
    }
}

impl ColorSwatches {
    /// Parses a [`ColorSwatches`] definition from a RON string.
    pub fn from_ron_str(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| format!("Failed to parse RON palette: {e}"))
    }

    /// Serializes this swatch table into a formatted RON string.
    pub fn to_ron_string(&self) -> Result<String, String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("Failed to serialize RON palette: {e}"))
    }

    /// Writes this swatch table to `<dir>/<file_stem>.ron`.
    pub fn save_to_dir(&self, dir: impl AsRef<Path>, file_stem: &str) -> Result<PathBuf, String> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
        let path = dir.join(format!("{file_stem}.ron"));
        let text = self.to_ron_string()?;
        std::fs::write(&path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
        Ok(path)
    }

    /// Loads all `*.ron` palette files in `dir`, falling back to [`Self::default()`] if none are found.
    pub fn load_dir(dir: impl AsRef<Path>) -> Vec<Self> {
        let mut palettes: Vec<Self> = std::fs::read_dir(dir.as_ref())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("ron"))
            .filter_map(|path| std::fs::read_to_string(&path).ok()?.parse_ron_logged(&path))
            .collect();

        if palettes.is_empty() {
            palettes.push(Self::default());
        } else {
            let default_name = Self::default().name;
            palettes.sort_by(|a, b| (a.name != default_name).cmp(&(b.name != default_name)).then(a.name.cmp(&b.name)));
        }
        palettes
    }
}

trait ParseRonExt {
    fn parse_ron_logged(&self, path: &Path) -> Option<ColorSwatches>;
}

impl ParseRonExt for str {
    fn parse_ron_logged(&self, path: &Path) -> Option<ColorSwatches> {
        match ColorSwatches::from_ron_str(self) {
            Ok(swatches) => Some(swatches),
            Err(e) => {
                eprintln!("Warning: skipping invalid palette {}: {e}", path.display());
                None
            }
        }
    }
}

/// Continuous color ramp (`t in [0, 1] -> Color32`), where the stops define gradients.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ColorMap {
    pub stops: Vec<Color32>,
}

impl Default for ColorMap {
    fn default() -> Self {
        Self::from_stops(ColorSwatches::default().mobility_stops)
    }
}

impl ColorMap {
    /// Multi-stop colormap from a slice or vector of stops.
    pub fn from_stops(stops: impl Into<Vec<Color32>>) -> Self {
        Self { stops: stops.into() }
    }

    /// Simple two-color linear gradient from `start` (`t = 0`) to `end` (`t = 1`).
    pub fn gradient(start: Color32, end: Color32) -> Self {
        Self { stops: vec![start, end] }
    }

    /// Three-stop diverging ramp (e.g. negative -> neutral -> positive charge density).
    pub fn diverging(low: Color32, mid: Color32, high: Color32) -> Self {
        Self { stops: vec![low, mid, high] }
    }

    /// Samples the continuous color ramp at normalized `t` in `[0, 1]`.
    pub fn sample(&self, t: f32) -> Color32 {
        match self.stops.as_slice() {
            [] => Color32::BLACK,
            [only] => *only,
            stops => {
                let scaled = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
                let idx = (scaled.floor() as usize).min(stops.len() - 2);
                lerp_rgb(stops[idx], stops[idx + 1], scaled - idx as f32)
            }
        }
    }

    /// Maps `value` linearly from `[val_at_0, val_at_1]` (supports inverted ranges) into `[0, 1]` and samples.
    pub fn sample_range(&self, value: f32, val_at_0: f32, val_at_1: f32) -> Color32 {
        let span = val_at_1 - val_at_0;
        let t = if span.abs() < 1e-6 { 0.5 } else { (value - val_at_0) / span };
        self.sample(t)
    }

    /// Samples `N` evenly spaced swatch colors across `[0, 1]` for legends.
    pub fn swatches<const N: usize>(&self) -> [Color32; N] {
        std::array::from_fn(|i| {
            let t = if N <= 1 { 0.5 } else { i as f32 / (N - 1) as f32 };
            self.sample(t)
        })
    }
}

/// Top-level collection of discrete domain palettes and continuous colormaps stored in [`crate::state::UiState`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Palette {
    pub active_name: String,
    #[serde(skip)]
    pub available: Vec<ColorSwatches>,
    pub sides: InterfaceSidePalette,
    pub contacts: ContactPalette,
    pub chemistry: ResidueChemistryPalette,
    pub elements: ElementPalette,
    pub mobility: ColorMap,
    pub charge: ColorMap,
    pub gizmo: GizmoPalette,
    pub chrome: ChromePalette,
}

impl Default for Palette {
    fn default() -> Self {
        let available = ColorSwatches::load_dir(PALETTES_DIR);
        let first = available.first().cloned().unwrap_or_default();
        let mut pal = Self {
            active_name: first.name.clone(),
            available,
            sides: InterfaceSidePalette::from_swatches(&first),
            contacts: ContactPalette::from_swatches(&first),
            chemistry: ResidueChemistryPalette::from_swatches(&first),
            elements: ElementPalette::from_swatches(&first),
            mobility: ColorMap::from_stops(first.mobility_stops.clone()),
            charge: ColorMap::diverging(first.red, Color32::WHITE, first.blue),
            gizmo: GizmoPalette::default(),
            chrome: ChromePalette::default(),
        };
        pal.apply_swatches(&first);
        pal
    }
}

impl Palette {
    /// Populates all molecular and lens palettes from `s`, preserving `gizmo` and `chrome`.
    pub fn apply_swatches(&mut self, s: &ColorSwatches) {
        self.active_name.clone_from(&s.name);
        self.sides = InterfaceSidePalette::from_swatches(s);
        self.contacts = ContactPalette::from_swatches(s);
        self.chemistry = ResidueChemistryPalette::from_swatches(s);
        self.elements = ElementPalette::from_swatches(s);
        self.mobility = ColorMap::from_stops(s.mobility_stops.clone());
        self.charge = ColorMap::diverging(s.red, Color32::WHITE, s.blue);
    }

    /// Applies the loaded palette matching `name`, if present in `available`.
    pub fn apply_by_name(&mut self, name: &str) {
        if let Some(s) = self.available.iter().find(|s| s.name == name).cloned() {
            self.apply_swatches(&s);
        }
    }

    /// Rescans [`PALETTES_DIR`] for `.ron` files.
    pub fn reload_available(&mut self) {
        self.available = ColorSwatches::load_dir(PALETTES_DIR);
    }
}

/// Base colors for interface sides and 3D boundary meshes (surfaces, sheets, envelopes, voids).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InterfaceSidePalette {
    pub side_a: Color32,
    pub side_b: Color32,
    pub unassigned: Color32,
    pub sheet: Color32,
    pub envelope_a: Color32,
    pub envelope_b: Color32,
    pub void: Color32,
}

impl Default for InterfaceSidePalette {
    fn default() -> Self {
        Self::from_swatches(&ColorSwatches::default())
    }
}

impl InterfaceSidePalette {
    /// Looks up interface side and mesh colors from a centralized [`ColorSwatches`] table.
    pub const fn from_swatches(s: &ColorSwatches) -> Self {
        Self {
            side_a: s.primary,
            side_b: s.secondary,
            unassigned: s.neutral,
            sheet: s.yellow,
            envelope_a: s.yellow,
            envelope_b: s.purple,
            void: s.cyan,
        }
    }

    /// Base color for an interface side.
    pub fn side(&self, side: InterfaceSide) -> Color32 {
        match side {
            InterfaceSide::SideA => self.side_a,
            InterfaceSide::SideB => self.side_b,
            InterfaceSide::None => self.unassigned,
        }
    }

    /// Dynamically lightened tint for an interface side (labels, spheres).
    pub fn side_light(&self, side: InterfaceSide) -> Color32 {
        match side {
            InterfaceSide::SideA | InterfaceSide::SideB => lighten(self.side(side), 0.22),
            InterfaceSide::None => self.unassigned,
        }
    }

    /// Dynamically darkened shade for an interface side (outlines, ribbons).
    pub fn side_dark(&self, side: InterfaceSide) -> Color32 {
        match side {
            InterfaceSide::SideA | InterfaceSide::SideB => darken(self.side(side), 0.20),
            InterfaceSide::None => self.unassigned,
        }
    }

    /// Contact envelope fill color for an interface side.
    pub fn envelope(&self, side: InterfaceSide) -> Color32 {
        match side {
            InterfaceSide::SideA => self.envelope_a,
            _ => self.envelope_b,
        }
    }
}

/// Colors for interatomic interactions and contact edges.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ContactPalette {
    /// Steric clashes (VWD radius overlaps) between atoms.
    pub clash: Color32,
    /// Salt bridges (electrostatic interactions) between atoms.
    pub salt_bridge: Color32,
    /// Hydrogen bonds (directional polar interactions) between atoms.
    pub hbond: Color32,
    /// Non-directional van der Waals contacts and same-residue edges.
    pub contact: Color32,
}

impl Default for ContactPalette {
    fn default() -> Self {
        Self::from_swatches(&ColorSwatches::default())
    }
}

impl ContactPalette {
    /// Looks up interaction colors from a centralized [`ColorSwatches`] table.
    pub const fn from_swatches(s: &ColorSwatches) -> Self {
        Self {
            clash: s.red,
            salt_bridge: s.purple,
            hbond: s.blue,
            contact: s.neutral,
        }
    }

    /// Color for a given [`EdgeKind`].
    pub fn color(&self, kind: EdgeKind) -> Color32 {
        match kind {
            EdgeKind::Clash => self.clash,
            EdgeKind::SaltBridge => self.salt_bridge,
            EdgeKind::HBond => self.hbond,
            EdgeKind::VanDerWaalsContact | EdgeKind::SameResidue => self.contact,
        }
    }
}

/// Colors amino acids using the residue chemistry lens.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ResidueChemistryPalette {
    pub aliphatic: Color32,
    pub aromatic: Color32,
    pub positive: Color32,
    pub negative: Color32,
    pub gly_pro: Color32,
    pub polar: Color32,
}

impl Default for ResidueChemistryPalette {
    fn default() -> Self {
        Self::from_swatches(&ColorSwatches::default())
    }
}

impl ResidueChemistryPalette {
    /// Looks up chemical category colors from a centralized [`ColorSwatches`] table.
    pub const fn from_swatches(s: &ColorSwatches) -> Self {
        Self {
            aliphatic: s.neutral,
            aromatic: s.orange,
            positive: s.blue,
            negative: s.red,
            gly_pro: s.purple,
            polar: s.cyan,
        }
    }
}

/// Colors for the elemental CPK lens.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ElementPalette {
    pub c: Color32,
    pub n: Color32,
    pub o: Color32,
    pub s: Color32,
    pub p: Color32,
    pub other: Color32,
}

impl Default for ElementPalette {
    fn default() -> Self {
        Self::from_swatches(&ColorSwatches::default())
    }
}

impl ElementPalette {
    /// Looks up elemental CPK colors from a centralized [`ColorSwatches`] table.
    pub const fn from_swatches(s: &ColorSwatches) -> Self {
        Self {
            c: s.carbon,
            n: s.blue,
            o: s.red,
            s: s.yellow,
            p: s.orange,
            other: s.neutral,
        }
    }

    /// Color for a chemical [`Element`].
    pub fn color(&self, el: Element) -> Color32 {
        match el {
            Element::C => self.c,
            Element::N => self.n,
            Element::O => self.o,
            Element::S => self.s,
            Element::P => self.p,
            _ => self.other,
        }
    }
}

/// Palette for the 3D orientation gizmo.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GizmoPalette {
    pub axis_u: Color32,
    pub axis_v: Color32,
    pub axis_n: Color32,
    pub bg_fill: Color32,
    pub bg_rim: Color32,
}

impl Default for GizmoPalette {
    fn default() -> Self {
        let d = ColorSwatches::default();
        Self {
            axis_u: d.red,
            axis_v: d.green,
            axis_n: d.blue,
            bg_fill: Color32::from_rgba_premultiplied(12, 18, 33, 198),
            bg_rim: Color32::from_rgba_premultiplied(52, 57, 65, 90),
        }
    }
}

impl GizmoPalette {
    /// Positive cap/arm color or dynamically darkened negative cap color for `axis`.
    pub fn axis_color(&self, axis: ViewAxis) -> Color32 {
        let primary = match axis {
            ViewAxis::PosU | ViewAxis::NegU => self.axis_u,
            ViewAxis::PosV | ViewAxis::NegV => self.axis_v,
            ViewAxis::PosN | ViewAxis::NegN => self.axis_n,
        };
        if axis.is_positive() { primary } else { darken(primary, 0.40) }
    }
}

/// Viewport background, ink, and semantic UI status colors.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChromePalette {
    /// Background color of the 3D viewport and 2D canvas.
    pub viewport_bg: Color32,
    /// Primary text and icon color for UI chrome.
    pub ink: Color32,
    /// Accent color for buttons, highlights, and interactive elements.
    pub accent: Color32,
    /// Warning color for UI status and alerts.
    pub warning: Color32,
    /// Success color for UI status and alerts.
    pub success: Color32,
    /// Error color for UI status and alerts.
    pub error: Color32,
}

impl Default for ChromePalette {
    fn default() -> Self {
        let d = ColorSwatches::default();
        Self {
            viewport_bg: Color32::from_rgb(248, 250, 252),
            ink: Color32::from_rgb(15, 23, 42),
            accent: d.primary,
            warning: d.orange,
            success: d.green,
            error: d.red,
        }
    }
}

impl ChromePalette {
    /// Dynamically derived muted caption color (blended between `ink` and `viewport_bg`).
    pub fn muted(&self) -> Color32 {
        lerp_rgb(self.ink, self.viewport_bg, 0.48)
    }

    /// Dynamically derived tangent-plane reference grid color.
    pub fn grid(&self) -> Color32 {
        lerp_rgb(self.viewport_bg, self.ink, 0.10)
    }

    /// Dynamically derived open-book hinge line color.
    pub fn hinge(&self) -> Color32 {
        self.muted()
    }

    /// Dynamically derived hover highlight ring color.
    pub fn hover_ring(&self) -> Color32 {
        highlight(self.accent)
    }
}

/// Lightens `c` by blending its RGB channels toward white by `amount` in `[0, 1]`.
pub fn lighten(c: Color32, amount: f32) -> Color32 {
    lerp_rgb(c, Color32::WHITE, amount)
}

/// Darkens `c` by scaling its RGB channels toward black by `amount` in `[0, 1]`.
pub fn darken(c: Color32, amount: f32) -> Color32 {
    let f = 1.0 - amount.clamp(0.0, 1.0);
    let s = |ch: u8| (f32::from(ch) * f).round() as u8;
    Color32::from_rgba_unmultiplied(s(c.r()), s(c.g()), s(c.b()), c.a())
}

/// Brightened highlight tint of `c`.
pub fn highlight(c: Color32) -> Color32 {
    lighten(c, 0.25)
}

/// Wireframe / border shade derived from a mesh or territory fill color.
pub fn edge_color(fill: Color32) -> Color32 {
    darken(fill, 0.32)
}

/// Linearly interpolates RGB channels between `a` and `b` by `t` in `[0, 1]`.
pub fn lerp_rgb(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Applies an opacity in `[0, 1]` to an opaque color.
pub fn with_alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Darkens an opaque color by a Lambert `shade` factor and applies opacity.
pub fn shaded(c: Color32, shade: f32, alpha: f32) -> Color32 {
    let s = |ch: u8| (f32::from(ch) * shade) as u8;
    with_alpha(Color32::from_rgb(s(c.r()), s(c.g()), s(c.b())), alpha)
}
