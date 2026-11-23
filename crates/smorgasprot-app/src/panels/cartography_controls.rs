//! Cartography ("Sandwich") tab controls: flatten and butterfly morphs, quick presets, layer customization popovers, and group visibility.

use super::widgets::{alpha_slider, channel_combo, choice_combo, section, tween_presets, tween_slider};
use crate::dataset::ViewerDataset;
use crate::state::{
    AtomSizeMode, ChannelPreset, Choice, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder, NodeStrokeColorLens,
    TerritoryColorLens, TerritoryStrokeLens, UiState, VisualChannel,
};
use egui::{Color32, RichText, Slider, Ui};
use smorgasprot_core::InterfaceSide;

/// Side B opacity below which the Open-Book preset raises it to fully opaque.
const OPEN_BOOK_MIN_B_ALPHA: f32 = 0.85;
/// Minimum width of a layer customization popover so dropdown labels fit cleanly.
const POPOVER_MIN_WIDTH: f32 = 290.0;

/// Renders the Sandwich (Cartography) tab's side panel.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    ui.heading("🥪 Sandwich Cartography");
    ui.add_space(4.0);
    let side_a_color = state.palette.sides.side_light(InterfaceSide::SideA);
    let side_b_color = state.palette.sides.side_light(InterfaceSide::SideB);
    let s = &mut state.cartography;

    section(ui, "1. 3D ↔ 2D Flattening (t)", |ui| {
        let t = s.flatten.value;
        let label = if t < 0.05 { "3D" } else if t > 0.95 { "2D Map" } else { "Bas-Relief" };
        tween_slider(ui, &mut s.flatten, label);
        tween_presets(ui, &mut s.flatten, &[("3D (0.0)", 0.0), ("Bas-Relief (0.5)", 0.5), ("2D Map (1.0)", 1.0)]);
        ui.checkbox(&mut s.use_relaxed_uv, "Use Spring-Relaxed 2D (vs. Tangent Plane)");
    });

    section(ui, "2. Overlay ↔ Open-Book Butterfly (θ)", |ui| {
        let t = s.book.value;
        let label = if t < 0.05 { "Overlay".to_owned() } else { format!("{}°", (t * 180.0).round() as i32) };
        tween_slider(ui, &mut s.book, label);
        let preset = tween_presets(ui, &mut s.book, &[("Overlay (0°)", 0.0), ("90°", 0.5), ("Open-Book (180°)", 1.0)]);
        // Opening the book exposes side B, so make sure it is visible.
        if preset == Some(1.0) && s.side_b_alpha.target < OPEN_BOOK_MIN_B_ALPHA {
            s.side_b_alpha.animate_to(1.0);
        }
    });

    section(ui, "3. Visual Lenses & Layer Channels", |ui| {
        ui.label(RichText::new("Quick Lens Presets:").strong());
        ui.horizontal_wrapped(|ui| {
            for &preset in ChannelPreset::ALL {
                if ui.button(preset.label()).clicked() {
                    s.apply_preset(preset);
                }
            }
        });

        ui.separator();

        // Atom Nodes layer row + customization popover
        ui.horizontal(|ui| {
            ui.label(RichText::new("Atom Nodes").strong());
            ui.weak(format!("• {}", s.atoms.fill.lens.label()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn_label = if s.atoms.is_default() { "⚙ Customize…" } else { "⚙ Customize*…" };
                ui.menu_button(btn_label, |ui| {
                    ui.set_min_width(POPOVER_MIN_WIDTH);
                    popover_header(ui, "Atom Nodes Layer", !s.atoms.is_default(), || s.atoms.reset());
                    color_channel_control(ui, "if_node_fill_lens", "Fill Color", &mut s.atoms.fill, ColorLens::Fixed);
                    scalar_channel_control(
                        ui,
                        "if_node_size_mode",
                        "Node Size (Radius)",
                        &mut s.atoms.radius,
                        AtomSizeMode::Fixed,
                        0.3..=3.0,
                        "× scale",
                    );
                    color_channel_control(
                        ui,
                        "if_node_stroke_color",
                        "Stroke (Border) Color",
                        &mut s.atoms.stroke_color,
                        NodeStrokeColorLens::Fixed,
                    );
                    scalar_channel_control(
                        ui,
                        "if_node_stroke_width",
                        "Stroke (Border) Width",
                        &mut s.atoms.stroke_width,
                        AtomSizeMode::Fixed,
                        0.5..=6.0,
                        "px",
                    );
                    ui.separator();
                    label_builder_control(
                        ui,
                        "if_atom_labels",
                        "Atom Labels",
                        &mut s.atoms.labels,
                        LabelBuilder::default_atom(),
                        true,
                    );
                });
            });
        });

        // Contact Edges layer row + customization popover
        ui.horizontal(|ui| {
            ui.label(RichText::new("Contact Edges").strong());
            ui.weak(format!("• {}", s.edges.color.lens.label()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn_label = if s.edges.is_default() { "⚙ Customize…" } else { "⚙ Customize*…" };
                ui.menu_button(btn_label, |ui| {
                    ui.set_min_width(POPOVER_MIN_WIDTH);
                    popover_header(ui, "Contact Edges Layer", !s.edges.is_default(), || s.edges.reset());
                    color_channel_control(ui, "if_edge_color_lens", "Edge Color", &mut s.edges.color, EdgeColorLens::Fixed);
                    scalar_channel_control(
                        ui,
                        "if_edge_width_lens",
                        "Edge Width",
                        &mut s.edges.width,
                        EdgeWidthLens::Fixed,
                        0.5..=6.0,
                        "px",
                    );
                });
            });
        });

        // Residue Territories layer row + customization popover
        ui.horizontal(|ui| {
            ui.label(RichText::new("Residue Territories").strong());
            ui.weak(format!("• {}", s.territories.fill.lens.label()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn_label = if s.territories.is_default() { "⚙ Customize…" } else { "⚙ Customize*…" };
                ui.menu_button(btn_label, |ui| {
                    ui.set_min_width(POPOVER_MIN_WIDTH);
                    popover_header(ui, "Residue Territories Layer", !s.territories.is_default(), || s.territories.reset());
                    color_channel_control(
                        ui,
                        "if_terr_color_lens",
                        "Territory Fill Color",
                        &mut s.territories.fill,
                        TerritoryColorLens::Fixed,
                    );
                    scalar_channel_control(
                        ui,
                        "if_terr_stroke_lens",
                        "Territory Border Width",
                        &mut s.territories.stroke_width,
                        TerritoryStrokeLens::Fixed,
                        0.5..=6.0,
                        "px",
                    );
                    ui.separator();
                    label_builder_control(
                        ui,
                        "if_territory_labels",
                        "Territory Labels",
                        &mut s.territories.labels,
                        LabelBuilder::default_territory(),
                        false,
                    );
                });
            });
        });

        ui.separator();
        ui.label("Mobility Column Interpretation:");
        choice_combo(ui, "if_bfactor_mode", &mut s.bfactor_mode);
    });

    section(ui, "4. Group Opacity & Layer Visibility", |ui| {
        let name_a = RichText::new(format!("{}:", ds.group_name(InterfaceSide::SideA))).color(side_a_color);
        let name_b = RichText::new(format!("{}:", ds.group_name(InterfaceSide::SideB))).color(side_b_color);
        alpha_slider(ui, name_a, &mut s.side_a_alpha);
        alpha_slider(ui, name_b, &mut s.side_b_alpha);
        ui.horizontal(|ui| {
            if ui.button("Solo A").clicked() {
                s.solo(InterfaceSide::SideA);
            }
            if ui.button("Solo B").clicked() {
                s.solo(InterfaceSide::SideB);
            }
            if ui.button("Both").clicked() {
                s.show_both();
            }
        });

        ui.separator();
        ui.checkbox(&mut s.territories.visible, "Residue Territory Contours");
        ui.checkbox(&mut s.edges.show_intra, "1st-Shell Intra-Chain Springs");
        ui.checkbox(&mut s.edges.show_cross, "Cross-Interface Contact Arcs");
        ui.checkbox(&mut s.show_backbone, "3D Cα Backbone Ribbons");
    });
}

/// Title row inside a layer customization popover with a layer-wide reset button.
fn popover_header(ui: &mut Ui, title: &str, modified: bool, on_reset: impl FnOnce()) {
    // `ui.menu_button` sets `button_frame = false` for flat menu items; restore normal widget frames inside popovers.
    ui.visuals_mut().button_frame = true;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        if modified {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("↺ Reset Layer").clicked() {
                    on_reset();
                }
            });
        }
    });
    ui.separator();
}

/// Color channel dropdown (`Fixed`, `Off`, divider, data lenses) + inline reset button + color picker when `Fixed`.
fn color_channel_control<T: Choice>(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    ch: &mut VisualChannel<T, Color32>,
    fixed_variant: T,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).strong());
        if !ch.is_default() && ui.small_button("↺").on_hover_text("Reset channel to default").clicked() {
            ch.reset();
        }
    });
    channel_combo(ui, id_salt, &mut ch.lens);
    if ch.lens == fixed_variant {
        ui.horizontal(|ui| {
            ui.label("Fixed Color:");
            ui.color_edit_button_srgba(&mut ch.fixed);
        });
    }
    ui.add_space(4.0);
}

/// Scalar channel dropdown (`Fixed`, `Off`, divider, data lenses) + inline reset button + numeric slider when `Fixed`.
fn scalar_channel_control<T: Choice>(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    ch: &mut VisualChannel<T, f32>,
    fixed_variant: T,
    range: std::ops::RangeInclusive<f32>,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).strong());
        if !ch.is_default() && ui.small_button("↺").on_hover_text("Reset channel to default").clicked() {
            ch.reset();
        }
    });
    channel_combo(ui, id_salt, &mut ch.lens);
    if ch.lens == fixed_variant {
        ui.add(Slider::new(&mut ch.fixed, range).step_by(0.05).text(unit));
    }
    ui.add_space(4.0);
}

/// Composable label builder inside a layer customization popover, with reset-to-default.
fn label_builder_control(
    ui: &mut Ui,
    id_salt: &str,
    title: &str,
    builder: &mut LabelBuilder,
    default_builder: LabelBuilder,
    is_atom: bool,
) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut builder.enabled, RichText::new(title).strong());
        if *builder != default_builder && ui.small_button("↺").on_hover_text("Reset labels to default").clicked() {
            *builder = default_builder;
        }
    });
    if builder.enabled {
        ui.indent(id_salt, |ui| {
            ui.label("Residue Name:");
            choice_combo(ui, &format!("{id_salt}_res_name"), &mut builder.res_name_style);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut builder.show_res_num, "Residue #");
                ui.checkbox(&mut builder.show_chain, "Chain ID");
                if is_atom {
                    ui.checkbox(&mut builder.show_atom_name, "Atom Name");
                } else {
                    ui.checkbox(&mut builder.show_sec_struct, "2° Struct");
                }
            });
            ui.label("Data Value Channel:");
            choice_combo(ui, &format!("{id_salt}_value_ch"), &mut builder.value_channel);
        });
    }
}
