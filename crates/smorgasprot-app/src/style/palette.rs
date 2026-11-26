//! Modular discrete palettes and continuous [`ColorMap`] gradients with self-healing `.ron` deserialization.
//! Every palette section has `#[serde(default)]`, so `.ron` files only need to specify the colors they override.

use crate::camera::ViewAxis;
use egui::Color32;
use serde::{Deserialize, Serialize};
use smorgasprot_core::{BondKind, EdgeKind, Element, InterfaceSide};
use std::path::{Path, PathBuf};

/// Default directory scanned for `.ron` palette files.
pub const PALETTES_DIR: &str = "assets/palettes";

/// Continuous color ramp (`t in [0, 1] -> Color32`), supporting 2-stop gradients and multi-stop colormaps.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ColorMap {
    pub stops: Vec<Color32>,
}

impl Default for ColorMap {
    fn default() -> Self {
        Self {
            stops: vec![
                Color32::from_rgb(0, 83, 214),
                Color32::from_rgb(101, 203, 243),
                Color32::from_rgb(255, 219, 19),
                Color32::from_rgb(255, 125, 69),
            ],
        }
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

/// Self-healing collection of domain palettes, general categorical colors, and continuous colormaps.
/// Any field omitted in a `.ron` file is automatically populated from [`Self::default()`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Palette {
    pub name: String,
    #[serde(skip)]
    pub available: Vec<Palette>,
    /// Arbitrary general-purpose categorical colors (for multi-chain assemblies, plots, and UI series).
    pub categorical: Vec<Color32>,
    pub sides: InterfaceSidePalette,
    pub contacts: ContactPalette,
    pub aa_chemistry: AaChemistryPalette,
    pub elements: ElementPalette,
    pub mobility: ColorMap,
    pub charge: ColorMap,
    pub thresholds: crate::state::LensParams,
    #[serde(skip)]
    pub gizmo: GizmoPalette,
    #[serde(skip)]
    pub chrome: ChromePalette,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            name: "Okabe-Ito (Colorblind-Safe)".into(),
            available: Vec::new(),
            categorical: vec![
                Color32::from_rgb(14, 165, 233),
                Color32::from_rgb(249, 115, 22),
                Color32::from_rgb(16, 185, 129),
                Color32::from_rgb(250, 204, 21),
                Color32::from_rgb(37, 99, 235),
                Color32::from_rgb(239, 68, 68),
                Color32::from_rgb(168, 85, 247),
                Color32::from_rgb(100, 116, 139),
            ],
            sides: InterfaceSidePalette::default(),
            contacts: ContactPalette::default(),
            aa_chemistry: AaChemistryPalette::default(),
            elements: ElementPalette::default(),
            mobility: ColorMap::default(),
            charge: ColorMap::diverging(
                Color32::from_rgb(239, 68, 68),
                Color32::WHITE,
                Color32::from_rgb(37, 99, 235),
            ),
            thresholds: crate::state::LensParams::default(),
            gizmo: GizmoPalette::default(),
            chrome: ChromePalette::default(),
        }
    }
}

impl Palette {
    /// Initializes the palette and loads all `.ron` presets found in [`PALETTES_DIR`].
    pub fn load_default() -> Self {
        let mut pal = Self::default();
        pal.reload_available();
        if let Some(first) = pal.available.first().cloned() {
            pal.apply_preset(&first);
        }
        pal
    }

    /// Parses a self-healing [`Palette`] from a RON string; missing fields fall back to [`Self::default()`].
    pub fn from_ron_str(text: &str) -> Result<Self, String> {
        let ext = ron::extensions::Extensions::UNWRAP_NEWTYPES | ron::extensions::Extensions::IMPLICIT_SOME;
        ron::Options::default()
            .with_default_extension(ext)
            .from_str(text)
            .map_err(|e| format!("Failed to parse RON palette: {e}"))
    }

    /// Serializes this palette into a formatted RON string.
    pub fn to_ron_string(&self) -> Result<String, String> {
        let ext = ron::extensions::Extensions::UNWRAP_NEWTYPES | ron::extensions::Extensions::IMPLICIT_SOME;
        let pretty = ron::ser::PrettyConfig::default().extensions(ext);
        ron::ser::to_string_pretty(self, pretty).map_err(|e| format!("Failed to serialize RON palette: {e}"))
    }

    /// Writes this palette to `<dir>/<file_stem>.ron`.
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
            .filter_map(|path| match std::fs::read_to_string(&path).ok().map(|s| Self::from_ron_str(&s))? {
                Ok(p) => Some(p),
                Err(e) => {
                    eprintln!("Warning: skipping invalid palette {}: {e}", path.display());
                    None
                }
            })
            .collect();

        if palettes.is_empty() {
            palettes.push(Self::default());
        } else {
            let default_name = Self::default().name;
            palettes.sort_by(|a, b| (a.name != default_name).cmp(&(b.name != default_name)).then(a.name.cmp(&b.name)));
        }
        palettes
    }

    /// Copies molecular and lens colors from `preset`, preserving `available`, `gizmo`, and `chrome`.
    pub fn apply_preset(&mut self, preset: &Self) {
        self.name.clone_from(&preset.name);
        self.categorical.clone_from(&preset.categorical);
        self.sides = preset.sides.clone();
        self.contacts = preset.contacts.clone();
        self.aa_chemistry = preset.aa_chemistry.clone();
        self.elements = preset.elements.clone();
        self.mobility = preset.mobility.clone();
        self.charge = preset.charge.clone();
        self.thresholds = preset.thresholds.clone();
    }

    /// Applies the loaded palette matching `name`, if present in `available`.
    pub fn apply_by_name(&mut self, name: &str) {
        if let Some(p) = self.available.iter().find(|p| p.name == name).cloned() {
            self.apply_preset(&p);
        }
    }

    /// Rescans [`PALETTES_DIR`] for `.ron` files.
    pub fn reload_available(&mut self) {
        self.available = Self::load_dir(PALETTES_DIR);
}

    /// General-purpose categorical color at `index` (wraps around if `index >= categorical.len()`).
    pub fn categorical(&self, index: usize) -> Color32 {
        if self.categorical.is_empty() {
            self.sides.unassigned
        } else {
            self.categorical[index % self.categorical.len()]
        }
    }
}

/// Purpose-named colors for interface sides and 3D boundary meshes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct InterfaceSidePalette {
    pub side_a: Color32,
    pub side_b: Color32,
    pub unassigned: Color32,
    pub sheet: Color32,
    pub envelope_a: Color32,
    pub envelope_b: Color32,
    pub void: Color32,
    pub cavity_buried: Color32,
    pub cavity_channel: Color32,
}

impl Default for InterfaceSidePalette {
    fn default() -> Self {
        Self {
            side_a: Color32::from_rgb(14, 165, 233),
            side_b: Color32::from_rgb(249, 115, 22),
            unassigned: Color32::from_rgb(148, 163, 184),
            sheet: Color32::from_rgb(250, 204, 21),
            envelope_a: Color32::from_rgb(250, 204, 21),
            envelope_b: Color32::from_rgb(168, 85, 247),
            void: Color32::from_rgb(6, 182, 212),
            cavity_buried: Color32::from_rgb(236, 72, 153),
            cavity_channel: Color32::from_rgb(45, 212, 191),
        }
    }
}

impl InterfaceSidePalette {
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

    /// Legend swatch entries `(color, label)` for the two interface groups.
    pub fn legend_entries(&self, group_a: &str, group_b: &str) -> Vec<(Color32, String)> {
        vec![(self.side_a, group_a.into()), (self.side_b, group_b.into())]
    }
}

/// Purpose-named colors for interatomic interactions and contact edges.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ContactPalette {
    /// Steric clashes (VWD radius overlaps) between atoms.
    pub clash: Color32,
    /// Salt bridges (electrostatic interactions) between atoms.
    pub salt_bridge: Color32,
    /// Hydrogen bonds (directional polar interactions) between atoms.
    pub hbond: Color32,
    /// Non-directional van der Waals contacts, sidechain, and other covalent edges.
    pub contact: Color32,
    /// Polypeptide backbone bonds (within a residue or the inter-residue peptide link).
    pub backbone: Color32,
    /// Cross-residue sulfur-sulfur disulfide bridges.
    pub disulfide: Color32,
}

impl Default for ContactPalette {
    fn default() -> Self {
        Self {
            clash: Color32::from_rgb(239, 68, 68),
            salt_bridge: Color32::from_rgb(168, 85, 247),
            hbond: Color32::from_rgb(37, 99, 235),
            contact: Color32::from_rgb(148, 163, 184),
            backbone: Color32::from_rgb(100, 116, 139),
            // Traditional sulfur-yellow, consistent with most molecular viewers' element coloring.
            disulfide: Color32::from_rgb(234, 179, 8),
        }
    }
}

impl ContactPalette {
    /// Color for a given [`EdgeKind`].
    pub fn color(&self, kind: EdgeKind) -> Color32 {
        match kind {
            EdgeKind::Clash => self.clash,
            EdgeKind::SaltBridge => self.salt_bridge,
            EdgeKind::HBond => self.hbond,
            EdgeKind::Covalent(BondKind::Backbone) => self.backbone,
            EdgeKind::Covalent(BondKind::Disulfide) => self.disulfide,
            EdgeKind::VanDerWaalsContact | EdgeKind::Covalent(BondKind::Sidechain | BondKind::Other) => self.contact,
        }
    }

    /// Legend swatch entries `(color, label)` for the Interactions lens, including group fallback colors.
    pub fn legend_entries(&self, sides: &InterfaceSidePalette, group_a: &str, group_b: &str) -> Vec<(Color32, String)> {
        vec![
            (self.clash, "VdW Clash".into()),
            (self.hbond, "H-Bond".into()),
            (self.salt_bridge, "Salt Bridge".into()),
            (self.disulfide, "Disulfide".into()),
            (sides.side_a, group_a.into()),
            (sides.side_light(InterfaceSide::SideB), group_b.into()),
        ]
    }
}

/// Purpose-named colors for the chemical category lens.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AaChemistryPalette {
    pub aliphatic: Color32,
    pub aromatic: Color32,
    pub positive: Color32,
    pub negative: Color32,
    pub gly_pro: Color32,
    pub polar: Color32,
}

impl Default for AaChemistryPalette {
    fn default() -> Self {
        Self {
            aliphatic: Color32::from_rgb(148, 163, 184),
            aromatic: Color32::from_rgb(245, 158, 11),
            positive: Color32::from_rgb(37, 99, 235),
            negative: Color32::from_rgb(239, 68, 68),
            gly_pro: Color32::from_rgb(168, 85, 247),
            polar: Color32::from_rgb(6, 182, 212),
        }
    }
}

impl AaChemistryPalette {
    /// Legend swatch entries `(color, label)` for the AA Chemistry lens.
    pub fn legend_entries(&self) -> Vec<(Color32, String)> {
        vec![
            (self.aliphatic, "Aliphatic".into()),
            (self.aromatic, "Aromatic".into()),
            (self.polar, "Polar".into()),
            (self.positive, "Positive".into()),
            (self.negative, "Negative".into()),
            (self.gly_pro, "Gly / Pro".into()),
        ]
    }
}

/// Dedicated elemental CPK palette covering biological and crystallographic elements.
/// Self-healing via `#[serde(default)]`: `.ron` files may override only `c` and inherit the rest.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ElementPalette {
    // Organic & backbone
    pub h: Color32,
    pub c: Color32,
    pub n: Color32,
    pub o: Color32,
    pub s: Color32,
    pub p: Color32,
    pub se: Color32,
    // Alkali & alkaline earth ions
    pub na: Color32,
    pub k: Color32,
    pub mg: Color32,
    pub ca: Color32,
    // First-row transition metals & cofactors
    pub mn: Color32,
    pub fe: Color32,
    pub co: Color32,
    pub ni: Color32,
    pub cu: Color32,
    pub zn: Color32,
    // Halogens
    pub f: Color32,
    pub cl: Color32,
    pub br: Color32,
    pub i: Color32,
    // Fallback
    pub other: Color32,
}

impl Default for ElementPalette {
    fn default() -> Self {
        Self {
            h: Color32::from_rgb(226, 232, 240),
            c: Color32::from_rgb(71, 85, 105),
            n: Color32::from_rgb(37, 99, 235),
            o: Color32::from_rgb(220, 38, 38),
            s: Color32::from_rgb(217, 119, 6),
            p: Color32::from_rgb(234, 88, 12),
            se: Color32::from_rgb(217, 145, 20),
            na: Color32::from_rgb(139, 92, 246),
            k: Color32::from_rgb(147, 51, 234),
            mg: Color32::from_rgb(34, 197, 94),
            ca: Color32::from_rgb(16, 185, 129),
            mn: Color32::from_rgb(153, 122, 199),
            fe: Color32::from_rgb(224, 102, 51),
            co: Color32::from_rgb(240, 144, 160),
            ni: Color32::from_rgb(80, 208, 80),
            cu: Color32::from_rgb(200, 128, 51),
            zn: Color32::from_rgb(125, 128, 176),
            f: Color32::from_rgb(132, 204, 22),
            cl: Color32::from_rgb(22, 163, 74),
            br: Color32::from_rgb(166, 41, 41),
            i: Color32::from_rgb(107, 33, 168),
            other: Color32::from_rgb(100, 116, 139),
        }
    }
}

impl ElementPalette {
    /// Color for a chemical [`Element`].
    pub fn color(&self, el: Element) -> Color32 {
        match el {
            Element::H => self.h,
            Element::C => self.c,
            Element::N => self.n,
            Element::O => self.o,
            Element::S => self.s,
            Element::P => self.p,
            Element::Se => self.se,
            Element::Na => self.na,
            Element::K => self.k,
            Element::Mg => self.mg,
            Element::Ca => self.ca,
            Element::Mn => self.mn,
            Element::Fe => self.fe,
            Element::Co => self.co,
            Element::Ni => self.ni,
            Element::Cu => self.cu,
            Element::Zn => self.zn,
            Element::F => self.f,
            Element::Cl => self.cl,
            Element::Br => self.br,
            Element::I => self.i,
            _ => self.other,
        }
    }

    /// Core organic element legend swatch entries `(color, label)` for the Modern CPK lens.
    pub fn legend_entries(&self) -> Vec<(Color32, String)> {
        vec![
            (self.c, "Carbon (C)".into()),
            (self.n, "Nitrogen (N)".into()),
            (self.o, "Oxygen (O)".into()),
            (self.s, "Sulfur (S)".into()),
        ]
    }
}

/// Dedicated palette for the 3D orientation gizmo (kept invariant across molecular palette changes).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct GizmoPalette {
    pub axis_u: Color32,
    pub axis_v: Color32,
    pub axis_n: Color32,
    pub bg_fill: Color32,
    pub bg_rim: Color32,
}

impl Default for GizmoPalette {
    fn default() -> Self {
        Self {
            axis_u: Color32::from_rgb(239, 68, 68),
            axis_v: Color32::from_rgb(34, 197, 94),
            axis_n: Color32::from_rgb(37, 99, 235),
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

/// Viewport background, ink, and semantic UI status colors (kept invariant across molecular palette changes).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ChromePalette {
    pub viewport_bg: Color32,
    pub ink: Color32,
    pub accent: Color32,
    pub warning: Color32,
    pub success: Color32,
    pub error: Color32,
}

impl Default for ChromePalette {
    fn default() -> Self {
        Self {
            viewport_bg: Color32::from_rgb(248, 250, 252),
            ink: Color32::from_rgb(15, 23, 42),
            accent: Color32::from_rgb(14, 165, 233),
            warning: Color32::from_rgb(245, 158, 11),
            success: Color32::from_rgb(34, 197, 94),
            error: Color32::from_rgb(239, 68, 68),
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
