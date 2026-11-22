//! Visual encoding: modular domain palettes, continuous colormaps, dynamic tinting/shading, color lenses, and atom sizing.
//! Maps user choices from `state` to concrete colors and pixel sizes.

pub mod lenses;
pub mod palette;
pub mod sizing;

pub use palette::{
    AaChemistryPalette, ChromePalette, ColorMap, ContactPalette, ElementPalette, GizmoPalette, InterfaceSidePalette,
    PALETTES_DIR, Palette,
};
