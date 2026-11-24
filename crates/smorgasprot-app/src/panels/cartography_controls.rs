//! Cartography ("Sandwich") tab controls: flatten and butterfly morphs, quick presets, layer
//! customization popovers, and group visibility.

use super::widgets::{
    EDGE_WIDTH_RANGE, RADIUS_RANGE, UNIT_RANGE, bare_slider, choice_combo, choice_combo_short, color_channel_control,
    customize_popover, max_text_width, popover_header, row_height, scalar_channel_control, section, truncate_paren,
    tween_presets, tween_slider,
};
use crate::dataset::ViewerDataset;
use crate::state::{
    AtomSizeMode, ChannelPreset, Choice, ColorLens, EdgeColorLens, EdgeWidthLens, LabelBuilder, NodeStrokeColorLens,
    TerritoryColorLens, TerritoryStrokeLens, Tween, UiState,
};
use egui::{Align, Grid, Layout, RichText, Ui, vec2};
use smorgasprot_core::InterfaceSide;

/// Renders the Sandwich (Cartography) tab's side panel.
pub fn show(ui: &mut Ui, ds: &ViewerDataset, state: &mut UiState) {
    ui.heading("Sandwich Cartography");
    ui.add_space(4.0);
    let side_a_color = state.palette.sides.side_light(InterfaceSide::SideA);
    let side_b_color = state.palette.sides.side_light(InterfaceSide::SideB);
    let open_min_b_alpha = state.view_params.morph.open_sandwich_min_b_alpha;
    let s = &mut state.cartography;

    section(ui, "1. 3D ↔ 2D Flattening", |ui| {
        let t = s.flatten.value;
        let label = if t < 0.05 { "3D" } else if t > 0.95 { "2D map" } else { "bas-relief" };
        tween_slider(ui, &mut s.flatten, label);
        tween_presets(ui, &mut s.flatten, &[("3D (0.0)", 0.0), ("Bas-relief (0.5)", 0.5), ("2D map (1.0)", 1.0)]);
        ui.checkbox(&mut s.use_relaxed_uv, "Spring-relaxed 2D (vs. tangent plane)");
    });

    section(ui, "2. Overlay ↔ Open sandwich (θ)", |ui| {
        let t = s.book.value;
        let label = if t < 0.05 { "Overlay".to_owned() } else { format!("{}°", (t * 180.0).round() as i32) };
        tween_slider(ui, &mut s.book, label);
        let preset = tween_presets(ui, &mut s.book, &[("Overlay (0°)", 0.0), ("Tilt (45°)", 0.25), ("Open (180°)", 1.0)]);
        // Opening the sandwich exposes side B, so make sure it is visible.
        if preset == Some(1.0) && s.side_b_alpha.target < open_min_b_alpha {
            s.side_b_alpha.animate_to(1.0);
        }
    });

    section(ui, "3. Visual lenses & layer channels", |ui| {
        ui.label(RichText::new("Quick presets:").strong());
        ui.horizontal_wrapped(|ui| {
            for &preset in ChannelPreset::ALL {
                if ui.button(preset.label()).clicked() {
                    s.apply_preset(preset);
                }
            }
        });

        ui.separator();

        let col1_w = max_text_width(ui, &["Atom nodes", "Contact edges", "Residue territories"], true);
        let col3_w = max_text_width(ui, &["Customize...", "Customize*..."], false) + 2.0 * ui.spacing().button_padding.x;

        layer_channel_row(
            ui,
            col1_w,
            col3_w,
            "Atom nodes",
            truncate_paren(s.atoms.fill.lens.label()),
            "if_atoms_popover",
            !s.atoms.is_default(),
            |ui| {
                popover_header(ui, false, "Atom nodes layer", !s.atoms.is_default(), || s.atoms.reset());
                color_channel_control(ui, "if_node_fill_lens", "Fill color", &mut s.atoms.fill, ColorLens::Fixed);
                scalar_channel_control(
                    ui,
                    "if_node_size_mode",
                    "Node radius",
                    &mut s.atoms.radius,
                    AtomSizeMode::Fixed,
                    RADIUS_RANGE,
                    "x scale",
                );
                color_channel_control(
                    ui,
                    "if_node_stroke_color",
                    "Border color",
                    &mut s.atoms.stroke_color,
                    NodeStrokeColorLens::Fixed,
                );
                scalar_channel_control(
                    ui,
                    "if_node_stroke_width",
                    "Border width",
                    &mut s.atoms.stroke_width,
                    AtomSizeMode::Fixed,
                    EDGE_WIDTH_RANGE,
                    "px",
                );
                ui.separator();
                label_builder_control(
                    ui,
                    "if_atom_labels",
                    "Labels",
                    &mut s.atoms.labels,
                    LabelBuilder::default_atom(),
                    true,
                );
            },
        );

        layer_channel_row(
            ui,
            col1_w,
            col3_w,
            "Contact edges",
            truncate_paren(s.edges.color.lens.label()),
            "if_edges_popover",
            !s.edges.is_default(),
            |ui| {
                popover_header(ui, false, "Contact edges layer", !s.edges.is_default(), || s.edges.reset());
                color_channel_control(ui, "if_edge_color_lens", "Edge color", &mut s.edges.color, EdgeColorLens::Fixed);
                scalar_channel_control(
                    ui,
                    "if_edge_width_lens",
                    "Edge width",
                    &mut s.edges.width,
                    EdgeWidthLens::Fixed,
                    EDGE_WIDTH_RANGE,
                    "px",
                );
            },
        );

        layer_channel_row(
            ui,
            col1_w,
            col3_w,
            "Residue territories",
            truncate_paren(s.territories.fill.lens.label()),
            "if_terr_popover",
            !s.territories.is_default(),
            |ui| {
                popover_header(
                    ui,
                    false,
                    "Residue territories layer",
                    !s.territories.is_default(),
                    || s.territories.reset(),
                );
                color_channel_control(
                    ui,
                    "if_terr_color_lens",
                    "Territory fill color",
                    &mut s.territories.fill,
                    TerritoryColorLens::Fixed,
                );
                scalar_channel_control(
                    ui,
                    "if_terr_stroke_lens",
                    "Territory border width",
                    &mut s.territories.stroke_width,
                    TerritoryStrokeLens::Fixed,
                    EDGE_WIDTH_RANGE,
                    "px",
                );
                ui.separator();
                label_builder_control(
                    ui,
                    "if_territory_labels",
                    "Territory labels",
                    &mut s.territories.labels,
                    LabelBuilder::default_territory(),
                    false,
                );
            },
        );

        ui.separator();
        ui.label("Mobility column interpretation:");
        choice_combo_short(ui, "if_bfactor_mode", &mut s.bfactor_mode);
    });

    section(ui, "4. Group visibility", |ui| {
        let name_a = RichText::new(ds.group_name(InterfaceSide::SideA)).color(side_a_color);
        let name_b = RichText::new(ds.group_name(InterfaceSide::SideB)).color(side_b_color);
        Grid::new("carto_group_visibility_grid")
            .num_columns(3)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                group_visibility_row(ui, name_a, &mut s.side_a_alpha);
                group_visibility_row(ui, name_b, &mut s.side_b_alpha);
            });

        ui.separator();
        ui.checkbox(&mut s.territories.visible, "Residue territory contours");
        ui.checkbox(&mut s.edges.show_intra, "1st-shell intra-chain contacts");
        ui.checkbox(&mut s.edges.show_cross, "Cross-interface contacts");
        ui.checkbox(&mut s.show_backbone, "3D Cα backbone ribbons");
        // A pure presentation choice — doesn't touch `InterfacePartition`'s actual chain-to-side assignment.
        let on_top_name = ds.group_name(if s.side_a_on_top { InterfaceSide::SideA } else { InterfaceSide::SideB });
        ui.checkbox(&mut s.side_a_on_top, format!("{on_top_name} draws on top"));
    });
}

#[allow(clippy::too_many_arguments)]
fn layer_channel_row(
    ui: &mut Ui,
    col1_w: f32,
    col3_w: f32,
    name: &str,
    summary: &str,
    popover_id: &str,
    modified: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        let h = row_height(ui);
        let spacing = ui.spacing().item_spacing.x;
        ui.allocate_ui_with_layout(vec2(col1_w, h), Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new(name).strong());
        });
        let mid_w = (ui.available_width() - col3_w - spacing).max(0.0);
        ui.allocate_ui_with_layout(vec2(mid_w, h), Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(summary).weak()).truncate());
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            customize_popover(ui, popover_id, modified, add_contents);
        });
    });
}

fn group_visibility_row(ui: &mut Ui, label: RichText, alpha: &mut Tween) {
    ui.label(label);
    bare_slider(ui, alpha, UNIT_RANGE);
    let mut visible = alpha.target > 0.01;
    if ui.checkbox(&mut visible, "").on_hover_text("Toggle group visibility").changed() {
        alpha.animate_to(if visible { 1.0 } else { 0.0 });
    }
    ui.end_row();
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
        if *builder != default_builder && ui.small_button("Reset").on_hover_text("Reset labels to default").clicked() {
            *builder = default_builder;
        }
    });
    if builder.enabled {
        ui.indent(id_salt, |ui| {
            ui.label("Residue name:");
            choice_combo(ui, &format!("{id_salt}_res_name"), &mut builder.res_name_style);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut builder.show_res_num, "Residue #");
                ui.checkbox(&mut builder.show_chain, "Chain ID");
                if is_atom {
                    ui.checkbox(&mut builder.show_atom_name, "Atom name");
                } else {
                    ui.checkbox(&mut builder.show_sec_struct, "Secondary structure");
                }
            });
            ui.label("Data value channel:");
            choice_combo(ui, &format!("{id_salt}_value_ch"), &mut builder.value_channel);
        });
    }
}
