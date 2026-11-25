//! Composable presentation styles, per-channel alphas, and capability flags shared across workspaces.

use super::Choice;
use egui::Color32;
use serde::{Deserialize, Serialize};
use smorgasprot_core::surface::GeodesicPreset;

/// Default slate ink for optional ChimeraX-style silhouette borders.
pub const DEFAULT_BORDER_COLOR: Color32 = Color32::from_rgb(30, 41, 59);

/// Which faces are culled on 3D meshes (`Surface`, `Cartoon` prism, `Boundary`, `Spheres`).
#[derive(Serialize, Deserialize, Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SurfaceCulling {
    /// Double-sided: no triangles culled.
    #[default]
    None,
    /// Hide backfacing triangles (solid closed shell).
    Backfaces,
    /// Hide frontfacing triangles (inside cutaway view).
    Frontfaces,
}

impl Choice for SurfaceCulling {
    const ALL: &'static [Self] = &[Self::None, Self::Backfaces, Self::Frontfaces];

    fn label(self) -> &'static str {
        match self {
            Self::None => "None (Double-Sided)",
            Self::Backfaces => "Backfaces (Solid Shell)",
            Self::Frontfaces => "Frontfaces (X-Ray Cutaway)",
        }
    }
}

impl Choice for GeodesicPreset {
    const ALL: &'static [Self] = &[
        Self::Octahedron8,
        Self::Icosahedron12,
        Self::Octahedron18,
        Self::Icosahedron42,
        Self::Octahedron66,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Octahedron8 => "Octahedron 8 (2.75 Å — mobile / minimal)",
            Self::Icosahedron12 => "Icosahedron 12 (2.20 Å — coarse)",
            Self::Octahedron18 => "Octahedron 18 (1.75 Å — fast display)",
            Self::Icosahedron42 => "Icosahedron 42 (1.45 Å — medium-fine)",
            Self::Octahedron66 => "Octahedron 66 (1.20 Å — fine)",
        }
    }
}

/// Per-object surface lighting style.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum MeshShading {
    /// Faceted Lambert lighting per triangle face normal.
    #[default]
    Flat,
    /// Interpolated vertex-normal lighting across triangles.
    Smooth,
    /// Flat unshaded base color (no directional lighting).
    Unlit,
}

impl Choice for MeshShading {
    const ALL: &'static [Self] = &[Self::Flat, Self::Smooth, Self::Unlit];

    fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat (faceted)",
            Self::Smooth => "Smooth (vertex normals)",
            Self::Unlit => "Unlit (flat color)",
        }
    }
}

/// Geometric representation of the Cα ribbon trace.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum RibbonStyle {
    #[default]
    Capsule2D,
    Prism8,
}

impl Choice for RibbonStyle {
    const ALL: &'static [Self] = &[Self::Capsule2D, Self::Prism8];

    fn label(self) -> &'static str {
        match self {
            Self::Capsule2D => "2D Capsule (smooth)",
            Self::Prism8 => "3D Prism (8-sided)",
        }
    }
}

/// Geometric complexity for DSSP Helix–Sheet–Loop protein cartoons.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CartoonQuality {
    Schematic,
    #[default]
    Detailed,
}

impl Choice for CartoonQuality {
    const ALL: &'static [Self] = &[Self::Schematic, Self::Detailed];

    fn label(self) -> &'static str {
        match self {
            Self::Schematic => "Schematic (Cylinders & Boxes)",
            Self::Detailed => "Detailed (Helical Ribbons & Sheet Arrows)",
        }
    }
}

/// Geometric complexity for Ball-and-Stick representations (2D or 3D impostors/meshes).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BallStickQuality {
    Flat2D,
    #[default]
    Impostor3D,
    MeshCoarse,
    MeshFine,
}

impl BallStickQuality {
    pub fn geodesic_preset(self) -> Option<GeodesicPreset> {
        match self {
            Self::Flat2D | Self::Impostor3D => None,
            Self::MeshCoarse => Some(GeodesicPreset::Octahedron18),
            Self::MeshFine => Some(GeodesicPreset::Icosahedron42),
        }
    }
}

impl Choice for BallStickQuality {
    const ALL: &'static [Self] = &[Self::Flat2D, Self::Impostor3D, Self::MeshCoarse, Self::MeshFine];

    fn label(self) -> &'static str {
        match self {
            Self::Flat2D => "2D Discs & Capsules",
            Self::Impostor3D => "3D Impostor Spheres & Cylinders",
            Self::MeshCoarse => "3D Mesh (Octahedron 18 + Cylinders)",
            Self::MeshFine => "3D Mesh (Icosahedron 42 + Cylinders)",
        }
    }
}

/// How covalent bond cylinders/capsules are colored relative to their endpoint atoms.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BondColorMode {
    #[default]
    SplitMidpoint,
    Gradient,
    Uniform,
}

impl Choice for BondColorMode {
    const ALL: &'static [Self] = &[Self::SplitMidpoint, Self::Gradient, Self::Uniform];

    fn label(self) -> &'static str {
        match self {
            Self::SplitMidpoint => "By atom (clean midpoint split)",
            Self::Gradient => "By atom (smooth gradient)",
            Self::Uniform => "Uniform fixed bond color",
        }
    }
}

/// Atom selection shortcut for Ball-and-Stick rendering.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BallStickScope {
    #[default]
    AllAtoms,
    SidechainsAndLigands,
    InterfaceSidechains,
    InterfaceResidues,
}

impl Choice for BallStickScope {
    const ALL: &'static [Self] = &[
        Self::AllAtoms,
        Self::SidechainsAndLigands,
        Self::InterfaceSidechains,
        Self::InterfaceResidues,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::AllAtoms => "All atoms (whole complex)",
            Self::SidechainsAndLigands => "Sidechains & ligands (anchor at Cα)",
            Self::InterfaceSidechains => "Active interface sidechains (anchor at Cα)",
            Self::InterfaceResidues => "Active interface residues (all atoms)",
        }
    }
}

/// Rendering quality level for atom spheres (billboard or real 3D geodesic mesh).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SphereQuality {
    Flat2D,
    #[default]
    Billboard3D,
    Octahedron8,
    Icosahedron12,
    Octahedron18,
    Icosahedron42,
}

impl SphereQuality {
    pub fn geodesic_preset(self) -> Option<GeodesicPreset> {
        match self {
            Self::Flat2D | Self::Billboard3D => None,
            Self::Octahedron8 => Some(GeodesicPreset::Octahedron8),
            Self::Icosahedron12 => Some(GeodesicPreset::Icosahedron12),
            Self::Octahedron18 => Some(GeodesicPreset::Octahedron18),
            Self::Icosahedron42 => Some(GeodesicPreset::Icosahedron42),
        }
    }
}

impl Choice for SphereQuality {
    const ALL: &'static [Self] = &[
        Self::Flat2D,
        Self::Billboard3D,
        Self::Octahedron8,
        Self::Icosahedron12,
        Self::Octahedron18,
        Self::Icosahedron42,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Flat2D => "2D Disc (flat)",
            Self::Billboard3D => "3D Billboard (smooth)",
            Self::Octahedron8 => "3D Octahedron 8",
            Self::Icosahedron12 => "3D Icosahedron 12",
            Self::Octahedron18 => "3D Octahedron 18",
            Self::Icosahedron42 => "3D Icosahedron 42",
        }
    }
}

/// Independent opacity multipliers for fill, silhouette border, and wireframe edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaChannels {
    pub fill: f32,
    pub border: f32,
    pub wireframe: f32,
}

impl Default for AlphaChannels {
    fn default() -> Self {
        Self { fill: 1.0, border: 1.0, wireframe: 1.0 }
    }
}

/// Silhouette display border styling (world-space width in Å).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderStyle {
    pub enabled: bool,
    pub width_a: f32,
    pub color: Color32,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self { enabled: false, width_a: 0.15, color: DEFAULT_BORDER_COLOR }
    }
}

impl BorderStyle {
    /// Screen-space border width in points at the given camera `zoom` (`px / Å`).
    pub fn effective_width_px(&self, zoom: f32) -> f32 {
        if self.enabled {
            (self.width_a * zoom).max(0.0)
        } else {
            0.0
        }
    }
}

/// Triangle wireframe overlay styling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireframeStyle {
    pub enabled: bool,
    pub width_px: f32,
}

impl Default for WireframeStyle {
    fn default() -> Self {
        Self { enabled: false, width_px: 1.2 }
    }
}

/// Composable presentation parameters shared by Complex, Garnish, and Interface representations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationStyle<Q = ()> {
    pub quality: Q,
    pub shading: MeshShading,
    pub culling: SurfaceCulling,
    pub border: BorderStyle,
    pub wireframe: WireframeStyle,
    pub alphas: AlphaChannels,
}

impl<Q: Default> Default for PresentationStyle<Q> {
    fn default() -> Self {
        Self {
            quality: Q::default(),
            shading: MeshShading::default(),
            culling: SurfaceCulling::default(),
            border: BorderStyle::default(),
            wireframe: WireframeStyle::default(),
            alphas: AlphaChannels::default(),
        }
    }
}

impl<Q: PartialEq> PresentationStyle<Q> {
    pub fn is_default_vs(&self, baseline: &Self) -> bool {
        self == baseline
    }
}

/// Feature flags controlling which controls appear in a representation's `Presentation` popover.
#[derive(Clone, Copy, Debug)]
pub struct PresentationCaps {
    pub shading: bool,
    pub culling: bool,
    pub border: bool,
    pub wireframe: bool,
    pub fill_alpha: bool,
}
