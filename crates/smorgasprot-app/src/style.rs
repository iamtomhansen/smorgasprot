//! Visual encoding: modular domain palettes, continuous colormaps, visual channel wrappers, color lenses, and sizing.
//! Maps user choices from `state` to concrete colors and pixel sizes across workspaces.

pub mod channels;
pub mod lenses;
pub mod palette;
pub mod sizing;

pub use channels::{
    AtomSizeMode, BFactorMode, ChannelPreset, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder, LabelValueChannel,
    NodeStrokeColorLens, ResidueNameStyle, ScalarMetric, TerritoryColorLens, TerritoryStrokeLens, VisualChannel,
};
pub use palette::{
    AaChemistryPalette, ChromePalette, ColorMap, ContactPalette, ElementPalette, GizmoPalette, InterfaceSidePalette,
    PALETTES_DIR, Palette,
};
