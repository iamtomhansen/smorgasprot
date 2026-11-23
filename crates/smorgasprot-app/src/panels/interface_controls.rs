//! Interface tab controls: flatten and butterfly morphs, multi-channel visual encodings, label builders, and per-side layers.

use super::widgets::{alpha_slider, channel_combo, choice_combo, section, tween_presets, tween_slider};
use crate::dataset::ViewerDataset;
use crate::state::{
    AtomSizeMode, Choice, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder, NodeStrokeColorLens,
    TerritoryColorLens, TerritoryStrokeLens, UiState,
};
use egui::{Color32, RichText, Slider, Ui};
use smorgasprot_core::InterfaceSide;

/// Side B opacity below which the Open-Book preset raises it to fully opaque.
const OPEN_BOOK_MIN_B_ALPHA: f32 = 0.85;

/// Renders the Interface tab's side panel.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    ui.heading("🗺️ 2D/3D Interface Cartography");
    ui.add_space(4.0);
    let side_a_color = state.palette.sides.side_light(InterfaceSide::SideA);
    let side_b_color = state.palette.sides.side_light(InterfaceSide::SideB);
    let s = &mut state.interface;

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

    section(ui, "3. Visual Channel Encodings", |ui| {
        ui.label(RichText::new("Atom Nodes").strong());
        color_channel_row(
            ui,
            "if_node_fill_lens",
            "Fill Color",
            &mut s.lens_enabled,
            &mut s.lens,
            ColorLens::Fixed,
            ColorLens::default(),
            &mut s.fixed_node_fill,
        );
        scalar_channel_row(
            ui,
            "if_node_size_mode",
            "Node Size (Radius)",
            &mut s.size_mode_enabled,
            &mut s.size_mode,
            AtomSizeMode::Fixed,
            AtomSizeMode::default(),
            &mut s.fixed_node_radius_scale,
            0.3..=3.0,
            "× scale",
        );
        color_channel_row(
            ui,
            "if_node_stroke_color",
            "Stroke (Border) Color",
            &mut s.stroke_color_enabled,
            &mut s.stroke_color_lens,
            NodeStrokeColorLens::Fixed,
            NodeStrokeColorLens::default(),
            &mut s.fixed_node_stroke_color,
        );
        scalar_channel_row(
            ui,
            "if_node_stroke_width",
            "Stroke (Border) Width",
            &mut s.stroke_width_enabled,
            &mut s.stroke_width_lens,
            AtomSizeMode::Fixed,
            AtomSizeMode::Fixed,
            &mut s.fixed_node_stroke_width_px,
            0.5..=6.0,
            "px",
        );
        label_builder_row(ui, "if_atom_labels", "Atom Labels", &mut s.atom_labels, true);

        ui.separator();
        ui.label(RichText::new("Contact Edges").strong());
        color_channel_row(
            ui,
            "if_edge_color_lens",
            "Edge Color",
            &mut s.edge_color_enabled,
            &mut s.edge_color_lens,
            EdgeColorLens::Fixed,
            EdgeColorLens::default(),
            &mut s.fixed_edge_color,
        );
        scalar_channel_row(
            ui,
            "if_edge_width_lens",
            "Edge Width",
            &mut s.edge_width_enabled,
            &mut s.edge_width_lens,
            EdgeWidthLens::Fixed,
            EdgeWidthLens::default(),
            &mut s.fixed_edge_width_px,
            0.5..=6.0,
            "px",
        );

        ui.separator();
        ui.label(RichText::new("Residue Territories").strong());
        color_channel_row(
            ui,
            "if_terr_color_lens",
            "Territory Fill Color",
            &mut s.territory_color_enabled,
            &mut s.territory_color_lens,
            TerritoryColorLens::Fixed,
            TerritoryColorLens::default(),
            &mut s.fixed_territory_fill,
        );
        scalar_channel_row(
            ui,
            "if_terr_stroke_lens",
            "Territory Border Width",
            &mut s.territory_stroke_enabled,
            &mut s.territory_stroke_lens,
            TerritoryStrokeLens::Fixed,
            TerritoryStrokeLens::Fixed,
            &mut s.fixed_territory_stroke_width_px,
            0.5..=6.0,
            "px",
        );
        label_builder_row(ui, "if_territory_labels", "Territory Labels", &mut s.territory_labels, false);

        ui.separator();
        ui.label("Mobility Column Interpretation:");
        choice_combo(ui, "if_bfactor_mode", &mut s.bfactor_mode);
    });

    section(ui, "4. Group Opacity & Visibility", |ui| {
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
        ui.checkbox(&mut s.show_territories, "Residue Territory Contours");
        ui.checkbox(&mut s.show_intra_edges, "1st-Shell Intra-Chain Springs");
        ui.checkbox(&mut s.show_cross_arcs, "Cross-Interface Contact Arcs");
        ui.checkbox(&mut s.show_backbone, "3D Cα Backbone Ribbons");
    });
}

/// Checkbox + optional dropdown (`Fixed`, `Off`, divider, data lenses) + color picker when `Fixed`.
#[allow(clippy::too_many_arguments)]
fn color_channel_row<T: Choice>(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    enabled: &mut bool,
    lens: &mut T,
    fixed_variant: T,
    fallback: T,
    fixed_color: &mut Color32,
) {
    ui.horizontal(|ui| {
        ui.checkbox(enabled, label);
        if !*enabled {
            ui.weak(format!("(Default: {})", fallback.label()));
        }
    });
    if *enabled {
        ui.indent(id_salt, |ui| {
            channel_combo(ui, id_salt, lens);
            if *lens == fixed_variant {
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    ui.color_edit_button_srgba(fixed_color);
                });
            }
        });
    }
}

/// Checkbox + optional dropdown (`Fixed`, `Off`, divider, data lenses) + numeric slider when `Fixed`.
#[allow(clippy::too_many_arguments)]
fn scalar_channel_row<T: Choice>(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    enabled: &mut bool,
    lens: &mut T,
    fixed_variant: T,
    fallback: T,
    fixed_value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.checkbox(enabled, label);
        if !*enabled {
            ui.weak(format!("(Default: {})", fallback.label()));
        }
    });
    if *enabled {
        ui.indent(id_salt, |ui| {
            channel_combo(ui, id_salt, lens);
            if *lens == fixed_variant {
                ui.add(Slider::new(fixed_value, range).step_by(0.05).text(unit));
            }
        });
    }
}

/// Composable label builder UI for either atom nodes or residue territories.
fn label_builder_row(ui: &mut Ui, id_salt: &str, title: &str, builder: &mut LabelBuilder, is_atom: bool) {
    ui.checkbox(&mut builder.enabled, title);
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
