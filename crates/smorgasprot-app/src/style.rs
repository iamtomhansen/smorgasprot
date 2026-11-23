//! Visual encoding: modular domain palettes, continuous colormaps, visual channel wrappers, color lenses, and sizing.
//! Maps user choices from `state` to concrete colors and pixel sizes across workspaces.

pub mod channels;
pub mod lenses;
pub mod palette;
pub mod sizing;

pub use channels::{
    AtomSizeMode, BFactorMode, ChainColorLens, ChannelPreset, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder,
    LabelValueChannel, NodeStrokeColorLens, ResidueNameStyle, ScalarMetric, TerritoryColorLens, TerritoryStrokeLens,
    VisualChannel,
};
pub use palette::{
    AaChemistryPalette, ChromePalette, ColorMap, ContactPalette, ElementPalette, GizmoPalette, InterfaceSidePalette,
    PALETTES_DIR, Palette,
};

/// Registers the bundled Inter variable font as the primary proportional UI font, keeping emoji/symbol fallbacks.
pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/Inter-VariableFont_opsz,wght.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".to_owned());
    ctx.set_fonts(fonts);
}
